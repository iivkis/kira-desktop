//! Permanent bottom Wayland Dock with allowlisted, GIO-backed app actions.
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use webkit6::prelude::*;
use crate::{calendar, launcher, native_ui, renderer_recovery};

#[derive(Debug, PartialEq, Eq)]
enum Command { Catalog, Open, Launch(usize), Icons(Vec<usize>) }

fn id(raw: &str) -> Option<usize> {
    if raw.is_empty() || !raw.bytes().all(|c| c.is_ascii_digit()) { return None }
    let value = raw.parse::<usize>().ok()?;
    (value < 1024).then_some(value)
}
fn parse(raw: &str) -> Option<Command> {
    if raw.len() > 160 { return None }
    match raw {
        "catalog" => return Some(Command::Catalog),
        "open" => return Some(Command::Open),
        _ => {}
    }
    if let Some(value) = raw.strip_prefix("launch:") {
        return Some(Command::Launch(id(value)?));
    }
    if let Some(value) = raw.strip_prefix("icons:") {
        let mut ids = Vec::new();
        for word in value.split(',') {
            let next = id(word)?;
            if ids.len() >= 8 || ids.contains(&next) { return None }
            ids.push(next);
        }
        if !ids.is_empty() { return Some(Command::Icons(ids)) }
    }
    None
}
fn favorites() -> Vec<String> {
    std::env::var("KIRA_DOCK_APPS").unwrap_or_default()
        .split(',').map(str::trim)
        .filter(|s| !s.is_empty() && s.len() <= 120 &&
            !s.chars().any(char::is_control))
        .take(8).map(str::to_owned).collect()
}
fn dispatch(view: &webkit6::WebView, event: &str, data: serde_json::Value) {
    let Ok(encoded) = serde_json::to_string(&data) else { return };
    let Ok(quoted) = serde_json::to_string(&encoded) else { return };
    let js = format!(
        "window.dispatchEvent(new CustomEvent('{event}', {{detail: JSON.parse({quoted})}}));"
    );
    view.evaluate_javascript(&js, None, None, None::<&gtk::gio::Cancellable>, |result| {
        if let Err(error) = result {
            eprintln!("kira-dock: WebKit event failed: {error}");
        }
    });
}
fn catalog_data(data: serde_json::Value, pins: &[String]) -> serde_json::Value {
    serde_json::json!({"apps":data["apps"].clone(),"favorites":pins})
}
pub fn start(app: &gtk::Application, html: &str) {
    let marker = "<html lang=\"en\">";
    if !html.contains(marker) { eprintln!("kira-dock: no document marker"); return }
    let document = html.replacen(marker,
        "<html lang=\"en\" data-kira-surface=\"dock\">", 1);
    let window = gtk::ApplicationWindow::new(app);
    window.set_title(Some("Kira Dock"));
    window.set_decorated(false);
    window.set_default_size(520, 72);
    window.init_layer_shell();
    window.set_namespace(Some("kira-dock"));
    window.set_layer(Layer::Top);
    window.set_anchor(Edge::Bottom, true);
    window.set_margin(Edge::Bottom, 12);
    window.set_exclusive_zone(0);
    window.set_keyboard_mode(KeyboardMode::OnDemand);

    let context = webkit6::WebContext::new();
    let manager = webkit6::UserContentManager::new();
    let view = webkit6::WebView::builder()
        .web_context(&context).user_content_manager(&manager).build();
    view.set_background_color(&gtk::gdk::RGBA::new(0.0, 0.0, 0.0, 1.0));

    let pins = Rc::new(favorites());
    let service = Rc::new(RefCell::new(match launcher::Catalog::discover() {
        Ok(catalog) => Some(catalog),
        Err(error) => { eprintln!("kira-dock: {error}"); None }
    }));
    let weak_view = view.downgrade();
    let commands = service.clone();
    let command_pins = pins.clone();
    let launcher_app = app.clone();
    let launcher_html = html.to_owned();
    manager.connect_script_message_received(Some("kiraDock"), move |_, incoming| {
        let raw = incoming.to_string();
        let Some(command) = parse(&raw) else { return };
        match command {
            Command::Open => calendar::open_launcher(&launcher_app, &launcher_html),
            Command::Catalog => {
                if let Some(view) = weak_view.upgrade() {
                    let borrowed = commands.borrow();
                    if let Some(data) = borrowed.as_ref().and_then(launcher::Catalog::ready_json) {
                        dispatch(&view, "kira:dock:apps",
                            catalog_data(data, command_pins.as_slice()));
                    } else if borrowed.is_none() {
                        dispatch(&view, "kira:dock:error",
                            serde_json::json!({"message":"Application service unavailable"}));
                    }
                }
            }
            Command::Launch(id) => {
                if let Some(catalog) = commands.borrow().as_ref() {
                    if let Err(error) = catalog.launch(id) {
                        if let Some(view) = weak_view.upgrade() {
                            dispatch(&view, "kira:dock:error",
                                serde_json::json!({"message":error}));
                        }
                    }
                }
            }
            Command::Icons(ids) => {
                if let Some(catalog) = commands.borrow().as_ref() {
                    if let Err(error) = catalog.request_icons(&ids) {
                        if let Some(view) = weak_view.upgrade() {
                            dispatch(&view, "kira:dock:error",
                                serde_json::json!({"message":error}));
                        }
                    }
                }
            }
        }
    });
    assert!(manager.register_script_message_handler("kiraDock", None));
    native_ui::restrict_webview_navigation(&view);

    let weak_view = view.downgrade();
    let poll = service.clone();
    let poll_pins = pins.clone();
    gtk::glib::timeout_add_local(Duration::from_millis(60), move || {
        let Some(view) = weak_view.upgrade() else {
            return gtk::glib::ControlFlow::Break;
        };
        if let Some(catalog) = poll.borrow_mut().as_mut() {
            for _ in 0..4 {
                let Some(event) = catalog.poll() else { break };
                match event {
                    launcher::LauncherMessage::Apps(data) =>
                        dispatch(&view, "kira:dock:apps", catalog_data(data, poll_pins.as_slice())),
                    launcher::LauncherMessage::Icons(data) =>
                        dispatch(&view, "kira:dock:icons", data),
                    launcher::LauncherMessage::Launched(Err(error))
                    | launcher::LauncherMessage::Error(error) =>
                        dispatch(&view, "kira:dock:error",
                            serde_json::json!({"message":error})),
                    launcher::LauncherMessage::Launched(Ok(())) => {}
                }
            }
        }
        gtk::glib::ControlFlow::Continue
    });
    // Bounded recovery, never a renderer crash/reload storm.
    let recovery = Rc::new(RefCell::new(renderer_recovery::RendererRecovery::default()));
    let retry_document = document.clone();
    view.connect_web_process_terminated(move |webview, reason| {
        use renderer_recovery::RetryDecision;
        match recovery.borrow_mut().on_terminated(Instant::now()) {
            RetryDecision::After(delay) => {
                eprintln!("kira-dock: renderer terminated ({reason:?}), retrying");
                let weak = webview.downgrade();
                let html = retry_document.clone();
                let recovery = recovery.clone();
                gtk::glib::timeout_add_local_once(delay, move || {
                    recovery.borrow_mut().retry_started();
                    if let Some(view) = weak.upgrade()
                        && view.root().is_some()
                    {
                        view.load_html(&html, None);
                    }
                });
            }
            RetryDecision::AlreadyPending => {}
            RetryDecision::Exhausted => eprintln!("kira-dock: renderer recovery exhausted"),
        }
    });
    view.load_html(&document, None);
    window.set_child(Some(&view));
    window.present();
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_bounded_native_app_indices_are_allowed() {
        assert_eq!(parse("catalog"), Some(Command::Catalog));
        assert_eq!(parse("open"), Some(Command::Open));
        assert_eq!(parse("launch:7"), Some(Command::Launch(7)));
        assert_eq!(parse("icons:0,9"), Some(Command::Icons(vec![0,9])));
        for bad in ["", "run:sh", "launch:-1", "launch:1024",
            "launch:1:2", "icons:", "icons:1,1",
            "icons:0,1,2,3,4,5,6,7,8"] {
            assert_eq!(parse(bad), None);
        }
    }
}

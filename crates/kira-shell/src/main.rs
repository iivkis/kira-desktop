//! GTK4/WebKitGTK host and bounded JSON-RPC bridge to the Core daemon.

mod calendar;
mod dock;
mod launcher;
mod native_ui;
mod process_monitor;
mod renderer_recovery;
mod system_metrics;
mod system_panel;
use std::cell::{Cell, RefCell};
use std::env;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use gtk::prelude::*;
use gtk4_layer_shell::{Edge, Layer, LayerShell};
use kira_ipc::{ConfigState, PROTOCOL_VERSION, RpcRequest, RpcResponse, WorkspaceState};
use webkit6::prelude::*;

const BAR_HEIGHT: i32 = 42;
const MAX_BRIDGE_MESSAGE_BYTES: usize = 4096;

/// Only the GTK main loop dispatches JS events; the worker threads contain
/// no GTK/WebKit handles.
enum ShellDelivery {
    Rpc(String),
    Workspaces(WorkspaceState),
    Config(ConfigState),
    CoreAvailability(bool),
    Metrics(system_metrics::Snapshot),
}

fn ui_index() -> PathBuf {
    let directory = env::var_os("KIRA_UI_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../frontend/dist"));
    directory.join("index.html")
}

// Never allow navigation away from the self-contained UI document.
fn permitted_ui_page(requested_uri: &str) -> bool {
    requested_uri == "about:blank"
}

fn core_socket() -> Option<PathBuf> {
    kira_ipc::core_socket_path().ok()
}

/// Expose read-only Core queries and one explicit, checked focus-workspace action.
fn process_message(message: &str, socket: Option<&Path>) -> RpcResponse {
    let request = match serde_json::from_str::<RpcRequest>(message) {
        Ok(value) => value,
        Err(_) => return RpcResponse::failure(None, -32700, "Invalid JSON-RPC message"),
    };
    if request.jsonrpc != PROTOCOL_VERSION {
        return RpcResponse::failure(Some(request.id), -32600, "Invalid JSON-RPC version");
    }
    if !matches!(
        request.method.as_str(),
        "ping" | "get_state" | "get_workspaces" | "get_config" | "focus_workspace"
    ) {
        return RpcResponse::failure(Some(request.id), -32601, "Method not allowed");
    }
    match socket {
        Some(path) => kira_ipc::client::call(path, &request).unwrap_or_else(|_| {
            RpcResponse::failure(Some(request.id), -32000, "Kira Core unavailable")
        }),
        None => RpcResponse::failure(Some(request.id), -32000, "XDG_RUNTIME_DIR is not set"),
    }
}

fn main() {
    let index = ui_index();
    if !index.is_file() {
        eprintln!(
            "Kira UI assets not found: {}. Run 'task frontend:build' first or set KIRA_UI_DIR.",
            index.display()
        );
        std::process::exit(1);
    }

    // Production assets are inlined into index.html. WebKit loads one
    // self-contained document without external script or stylesheet requests.
    let html = std::fs::read_to_string(&index).unwrap_or_else(|error| {
        eprintln!(
            "kira-shell: failed to read UI HTML {}: {error}",
            index.display()
        );
        std::process::exit(1);
    });

    // A nested preview must not activate another GTK application with the same ID
    // already running in the user's primary desktop session.
    let flags = if env::var_os("KIRA_PREVIEW").is_some() {
        gtk::gio::ApplicationFlags::NON_UNIQUE
    } else {
        gtk::gio::ApplicationFlags::empty()
    };
    let app = gtk::Application::builder()
        .application_id("dev.kira.Desktop")
        .flags(flags)
        .build();
    app.connect_activate(move |app| {
        let window = gtk::ApplicationWindow::new(app);
        window.set_title(Some("Kira Desktop"));
        window.set_decorated(false);
        window.set_default_size(800, BAR_HEIGHT);

        window.init_layer_shell();
        window.set_namespace(Some("kira-desktop"));
        window.set_layer(Layer::Top);
        window.set_anchor(Edge::Top, true);
        window.set_anchor(Edge::Left, true);
        window.set_anchor(Edge::Right, true);
        window.set_exclusive_zone(BAR_HEIGHT);

        // Keep a dedicated context for the locked-down, self-contained UI.
        let context = webkit6::WebContext::new();
        let manager = webkit6::UserContentManager::new();
        let view = webkit6::WebView::builder()
            .web_context(&context)
            .user_content_manager(&manager)
            .build();
        view.set_background_color(&gtk::gdk::RGBA::new(0.0, 0.0, 0.0, 1.0));

        // GTK and WebKit are not Send: socket IO runs on workers, UI updates
        // return to the GTK main loop through this channel.
        let (sender, receiver) = mpsc::channel::<ShellDelivery>();
        let socket = core_socket();
        let latest_metrics = Rc::new(RefCell::new(system_metrics::Snapshot::default()));
        let core_online = Rc::new(Cell::new(false));
        let niri_online = Rc::new(Cell::new(false));

        // Only cheap read-only kernel files are sampled off the GTK thread.
        // The monitor stops when the UI receiver has been dropped.
        let metrics_tx = sender.clone();
        std::thread::spawn(move || {
            let mut sampler = system_metrics::Sampler::default();
            loop {
                if metrics_tx.send(ShellDelivery::Metrics(sampler.sample())).is_err() {
                    break;
                }
                std::thread::sleep(Duration::from_secs(2));
            }
        });

        // One long-lived, read-only Core subscription. Core receives Niri
        // EventStream and immediately forwards changed snapshots to this
        // worker, avoiding the previous 1.5-second Svelte polling lag.
        if let Some(path) = socket.clone() {
            let updates = sender.clone();
            std::thread::spawn(move || {
                loop {
                    let result = kira_ipc::client::subscribe_workspaces(&path, |state| {
                        let _ = updates.send(ShellDelivery::CoreAvailability(true));
                        let _ = updates.send(ShellDelivery::Workspaces(state));
                    });
                    // Never show stale status after a Core stream disconnect.
                    let _ = updates.send(ShellDelivery::CoreAvailability(false));
                    let _ = updates.send(ShellDelivery::Workspaces(WorkspaceState::default()));
                    if let Err(error) = result {
                        eprintln!("kira-shell: Core workspace stream disconnected: {error}");
                    }
                    std::thread::sleep(Duration::from_millis(750));
                }
            });
        }
        // Core owns config validation and last-known-good rollback. A second
        // independent read-only stream forwards updates to the trusted page.
        if let Some(path) = socket.clone() {
            let updates = sender.clone();
            std::thread::spawn(move || {
                loop {
                    let result = kira_ipc::client::subscribe_config(&path, |state| {
                        let _ = updates.send(ShellDelivery::Config(state));
                    });
                    if let Err(error) = result {
                        eprintln!("kira-shell: Core config stream disconnected: {error}");
                    }
                    // Retain the last valid UI settings while Core reconnects.
                    std::thread::sleep(Duration::from_millis(750));
                }
            });
        }
        manager.connect_script_message_received(Some("kira"), move |_, value| {
            let message = value.to_string().to_string();
            let id = serde_json::from_str::<RpcRequest>(&message).ok().map(|r| r.id);
            let sender = sender.clone();
            let socket = socket.clone();
            std::thread::spawn(move || {
                let response = if message.len() > MAX_BRIDGE_MESSAGE_BYTES {
                    RpcResponse::failure(id, -32600, "Bridge message too large")
                } else {
                    process_message(&message, socket.as_deref())
                };
                if let Ok(serialized) = serde_json::to_string(&response) {
                    let _ = sender.send(ShellDelivery::Rpc(serialized));
                }
            });
        });
        assert!(manager.register_script_message_handler("kira", None));
        manager.connect_script_message_received(Some("kiraDiagnostics"), |_, value| {
            eprintln!("kira-shell [ui]: {}", value.to_string());
        });
        assert!(manager.register_script_message_handler("kiraDiagnostics", None));

        // A dedicated fixed-purpose message from the trusted, navigation-locked
        // local UI. Never forward launcher requests to Core or execute JS text.
        let launcher_application = app.clone();
        let launcher_html = html.clone();
        manager.connect_script_message_received(Some("kiraLauncher"), move |_, value| {
            if value.to_string() == "open" {
                calendar::open_launcher(&launcher_application, &launcher_html);
            }
        });
        assert!(manager.register_script_message_handler("kiraLauncher", None));

        // Separate, fixed-purpose UI control. Never interpret arbitrary JS
        // messages or expand the Core JSON-RPC method allowlist.
        let calendar_application = app.clone();
        let calendar_html = html.clone();
        manager.connect_script_message_received(Some("kiraCalendar"), move |_, value| {
            if value.to_string() == "toggle" {
                calendar::toggle(&calendar_application, &calendar_html);
            }
        });
        assert!(manager.register_script_message_handler("kiraCalendar", None));

        // Fixed-purpose system diagnostics toggle. GTK samples the kernel
        // read-only; the page can neither supply metric values nor commands.
        let system_app = app.clone();
        let system_stats = latest_metrics.clone();
        let system_core = core_online.clone();
        let system_niri = niri_online.clone();
        let system_html = html.clone();
        manager.connect_script_message_received(Some("kiraSystem"), move |_, value| {
            if value.to_string() == "toggle" {
                calendar::toggle_system(
                    &system_app,
                    &system_html,
                    system_stats.clone(),
                    system_core.clone(),
                    system_niri.clone(),
                );
            }
        });
        assert!(manager.register_script_message_handler("kiraSystem", None));

        view.connect_load_failed(|_, event, uri, error| {
            eprintln!("kira-shell: WebKit load failed ({event:?}) for {uri}: {error}");
            false
        });
        // The WebKit child can die while the GTK layer-shell window stays
        // alive. Reload the original trusted HTML after a bounded backoff;
        // never recreate the IPC/GTK handlers or silently navigate elsewhere.
        let bar_document = html.clone();
        let recovery = Rc::new(RefCell::new(renderer_recovery::RendererRecovery::default()));
        view.connect_web_process_terminated(move |webview, reason| {
            use renderer_recovery::RetryDecision;
            match recovery.borrow_mut().on_terminated(Instant::now()) {
                RetryDecision::After(delay) => {
                    eprintln!(
                        "kira-shell: Top Bar WebKit renderer terminated ({reason:?}); retrying in {} ms",
                        delay.as_millis()
                    );
                    let weak_view = webview.downgrade();
                    let document = bar_document.clone();
                    let recovery = recovery.clone();
                    gtk::glib::timeout_add_local_once(delay, move || {
                        recovery.borrow_mut().retry_started();
                        if let Some(webview) = weak_view.upgrade()
                            && webview.root().is_some()
                        {
                            webview.load_html(&document, None);
                        }
                    });
                }
                RetryDecision::AlreadyPending => {
                    eprintln!("kira-shell: Top Bar renderer recovery already scheduled");
                }
                RetryDecision::Exhausted => {
                    eprintln!(
                        "kira-shell: Top Bar renderer crashed repeatedly; restart kira-shell to recover"
                    );
                }
            }
        });
        view.connect_load_changed(|webview, event| {
            if event == webkit6::LoadEvent::Finished {
                let webview = webview.clone();
                gtk::glib::timeout_add_local_once(Duration::from_millis(1500), move || {
                    webview.evaluate_javascript(
                        "if (!document.querySelector('main[data-kira-ui-root]')) { throw new Error('Svelte UI did not mount; ' + 'boot=' + JSON.stringify(window.__kiraBoot || {}) + '; readyState=' + document.readyState + '; scripts=' + document.scripts.length); }",
                        None,
                        None,
                        None::<&gtk::gio::Cancellable>,
                        |result| {
                            if let Err(error) = result {
                                eprintln!("kira-shell: UI rendering check failed: {error}");
                            } else {
                                eprintln!("kira-shell: Svelte panel DOM verified");
                            }
                        },
                    );
                });
            }
        });

        let webview = view.clone();
        gtk::glib::timeout_add_local(Duration::from_millis(30), move || {
            while let Ok(delivery) = receiver.try_recv() {
                // Double encoding ensures payload content cannot become JS.
                let (event, payload) = match delivery {
                    ShellDelivery::Rpc(serialized) => ("kira:rpc", serialized),
                    ShellDelivery::Workspaces(state) => {
                        niri_online.set(state.connected);
                        let Ok(serialized) = serde_json::to_string(&state) else {
                            continue;
                        };
                        ("kira:workspaces", serialized)
                    }
                    ShellDelivery::Config(state) => {
                        let Ok(serialized) = serde_json::to_string(&state) else {
                            continue;
                        };
                        ("kira:config", serialized)
                    }
                    ShellDelivery::CoreAvailability(online) => {
                        core_online.set(online);
                        continue;
                    }
                    ShellDelivery::Metrics(mut snapshot) => {
                        // Gio queries the current root filesystem from the GTK
                        // thread, while /proc reads happen off-thread.
                        if let Some((total, available)) = system_panel::disk() {
                            snapshot.root_total_kib = Some(total);
                            snapshot.root_available_kib = Some(available);
                        }
                        *latest_metrics.borrow_mut() = snapshot.clone();
                        ("kira:system-metrics", snapshot.as_json().to_string())
                    }
                };
                if let Ok(encoded) = serde_json::to_string(&payload) {
                    let script = format!(
                        "window.dispatchEvent(new CustomEvent('{event}', {{ detail: JSON.parse({encoded}) }}));"
                    );
                    webview.evaluate_javascript(
                        &script,
                        None,
                        None,
                        None::<&gtk::gio::Cancellable>,
                        |result| {
                            if let Err(error) = result {
                                eprintln!("Kira bridge delivery error: {error}");
                            }
                        },
                    );
                }
            }
            gtk::glib::ControlFlow::Continue
        });

        native_ui::restrict_webview_navigation(&view);
        eprintln!("kira-shell: loading self-contained HTML using WebKit load_html()");
        view.load_html(&html, None);
        window.set_child(Some(&view));
        window.present();

        // Dock owns a distinct bottom Wayland surface and trusted GIO bridge.
        if env::var("KIRA_DOCK_DISABLE").ok().as_deref() != Some("1") {
            dock::start(app, &html);
        }

        // Warm the isolated Calendar WebKit process after the Top Bar starts.
        // The popup stays unmapped until requested; initial startup is not
        // blocked by the Calendar's JS initialization.
        let calendar_app = app.clone();
        let calendar_source = html.clone();
        gtk::glib::timeout_add_local_once(Duration::from_millis(200), move || {
            calendar::prewarm(&calendar_app, &calendar_source);
        });
    });
    app.run();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unsupported_method_without_core_connection() {
        let result = process_message(r#"{"jsonrpc":"2.0","id":9,"method":"execute_shell"}"#, None);
        assert_eq!(result.error.unwrap().code, -32601);
    }

    #[test]
    fn allows_read_only_workspace_method_but_reports_missing_core() {
        let result = process_message(
            r#"{"jsonrpc":"2.0","id":7,"method":"get_workspaces"}"#,
            None,
        );
        assert_eq!(result.error.unwrap().code, -32000);
    }

    #[test]
    fn allows_read_only_config_but_rejects_writes() {
        let response = process_message(r#"{"jsonrpc":"2.0","id":8,"method":"get_config"}"#, None);
        assert_eq!(response.error.unwrap().code, -32000);
        let response = process_message(r#"{"jsonrpc":"2.0","id":9,"method":"set_config"}"#, None);
        assert_eq!(response.error.unwrap().code, -32601);
        let response = process_message(
            r#"{"jsonrpc":"2.0","id":10,"method":"subscribe_config"}"#,
            None,
        );
        assert_eq!(response.error.unwrap().code, -32601);
    }

    #[test]
    fn allows_exact_focus_action_only_but_not_generic_niri_actions() {
        let focus = process_message(
            r#"{"jsonrpc":"2.0","id":4,"method":"focus_workspace","params":{"id":"2"}}"#,
            None,
        );
        // Accepted by the bridge, unavailable only because Core is absent.
        assert_eq!(focus.error.unwrap().code, -32000);
        for method in ["niri_action", "move_window", "focus_window", "set_config"] {
            let request = format!(r#"{{"jsonrpc":"2.0","id":5,"method":"{method}"}}"#);
            let result = process_message(&request, None);
            assert_eq!(result.error.unwrap().code, -32601);
        }
    }

    #[test]
    fn navigation_is_restricted_to_exact_ui_document() {
        assert!(permitted_ui_page("about:blank"));
        for uri in [
            "https://evil.example/",
            "kira-ui://app/index.html",
            "file:///etc/passwd",
        ] {
            assert!(!permitted_ui_page(uri));
        }
    }

    #[test]
    fn reports_missing_core() {
        let result = process_message(r#"{"jsonrpc":"2.0","id":3,"method":"ping"}"#, None);
        assert_eq!(result.error.unwrap().code, -32000);
    }
}

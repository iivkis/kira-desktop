//! Native GTK4/Wayland shell. No browser runtime or JavaScript bridge.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use gtk::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use kira_ipc::{ConfigState, PROTOCOL_VERSION, RpcRequest, UiConfig, WorkspaceState};

const BAR_HEIGHT: i32 = 42;
const POPUP_GAP: i32 = 12;

enum CoreUpdate {
    Workspaces(WorkspaceState),
    Config(ConfigState),
}

fn install_style() {
    let provider = gtk::CssProvider::new();
    provider.load_from_data(include_str!("style.css"));
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

fn request_focus(id: String) {
    // A widget never gets a generic Niri command. Core validates this ID
    // against its current snapshot before issuing the fixed FocusWorkspace action.
    std::thread::spawn(move || {
        if let Ok(path) = kira_ipc::core_socket_path() {
            let request = RpcRequest {
                jsonrpc: PROTOCOL_VERSION.to_owned(),
                id: 1,
                method: "focus_workspace".to_owned(),
                params: Some(serde_json::json!({ "id": id })),
            };
            if let Err(error) = kira_ipc::client::call(&path, &request) {
                eprintln!("kira-shell: workspace focus request failed: {error}");
            }
        }
    });
}

fn redraw_workspaces(container: &gtk::Box, state: &WorkspaceState, config: &UiConfig) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
    if !state.connected {
        let offline = gtk::Label::new(Some("NIRI OFFLINE"));
        offline.add_css_class("offline");
        container.append(&offline);
        return;
    }
    for workspace in state
        .workspaces
        .iter()
        .take(usize::from(config.max_workspaces))
    {
        let title = if config.show_names {
            workspace
                .name
                .clone()
                .unwrap_or_else(|| workspace.idx.to_string())
        } else {
            workspace.idx.to_string()
        };
        let button = gtk::Button::with_label(&title);
        button.add_css_class("workspace");
        if workspace.is_focused {
            button.add_css_class("selected");
        } else if workspace.is_urgent {
            button.add_css_class("urgent");
        }
        button.set_tooltip_text(Some(&format!("Workspace {}", workspace.idx)));
        let id = workspace.id.clone();
        button.connect_clicked(move |_| request_focus(id.clone()));
        container.append(&button);
    }
}

fn current_clock(show_seconds: bool) -> String {
    let format = if show_seconds { "%H:%M:%S" } else { "%H:%M" };
    gtk::glib::DateTime::now_local()
        .ok()
        .and_then(|time| time.format(format).ok())
        .map(|time| time.to_string())
        .unwrap_or_else(|| "--:--".to_owned())
}

fn make_button(symbol: &str, description: &str, on_click: impl Fn() + 'static) -> gtk::Button {
    let button = gtk::Button::with_label(symbol);
    button.add_css_class("bar-action");
    button.set_tooltip_text(Some(description));
    button.update_property(&[gtk::accessible::Property::Label(description)]);
    button.connect_clicked(move |_| on_click());
    button
}

fn render_applications(list: &gtk::Box, applications: &[gtk::gio::AppInfo], query: &str) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    let needle = query.trim().to_lowercase();
    let mut count = 0;
    for app in applications
        .iter()
        .filter(|app| app.should_show() && app.display_name().to_lowercase().contains(&needle))
    {
        if count >= 100 {
            break;
        }
        count += 1;
        let button = gtk::Button::with_label(&app.display_name());
        button.add_css_class("app-row");
        button.set_halign(gtk::Align::Fill);
        button.set_tooltip_text(Some(&app.display_name()));
        let app = app.clone();
        button.connect_clicked(move |_| {
            if let Err(error) = app.launch(&[], None::<&gtk::gio::AppLaunchContext>) {
                eprintln!("kira-shell: failed to launch application: {error}");
            }
        });
        list.append(&button);
    }
    if count == 0 {
        let empty = gtk::Label::new(Some("Приложений не найдено"));
        empty.add_css_class("muted");
        list.append(&empty);
    }
}

fn launcher_view() -> gtk::Box {
    let root = gtk::Box::new(gtk::Orientation::Vertical, 12);
    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some("Найти приложение"));
    search.update_property(&[gtk::accessible::Property::Label("Поиск приложений")]);
    root.append(&search);

    let scroll = gtk::ScrolledWindow::new();
    scroll.set_min_content_height(280);
    scroll.set_max_content_height(520);
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    let list = gtk::Box::new(gtk::Orientation::Vertical, 4);
    scroll.set_child(Some(&list));
    root.append(&scroll);

    let mut applications = gtk::gio::AppInfo::all();
    applications.sort_by_key(|app| app.display_name().to_lowercase());
    let applications = Rc::new(applications);
    render_applications(&list, &applications, "");
    search.connect_search_changed(move |entry| {
        render_applications(&list, &applications, &entry.text());
    });
    root
}

fn system_view() -> gtk::Box {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    let heading = gtk::Label::new(Some("Система"));
    heading.add_css_class("section-title");
    content.append(&heading);

    let info = gtk::Label::new(Some("Загрузка данных"));
    info.set_xalign(0.0);
    info.set_wrap(true);
    info.add_css_class("muted");
    content.append(&info);

    // Only short reads of procfs; no fake values and no background browser.
    // This initial view is a read-only migration baseline, not the old charts.
    let (sender, receiver) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        let memory = std::fs::read_to_string("/proc/meminfo")
            .ok()
            .and_then(|raw| {
                raw.lines()
                    .find(|line| line.starts_with("MemAvailable:"))
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| "ОЗУ: недоступно".to_owned());
        let load = std::fs::read_to_string("/proc/loadavg")
            .ok()
            .map(|raw| raw.split_whitespace().take(3).collect::<Vec<_>>().join(" "))
            .unwrap_or_else(|| "недоступно".to_owned());
        let _ = sender.send(format!("Нагрузка (1/5/15 мин): {load}\n{memory}"));
    });
    gtk::glib::timeout_add_local(Duration::from_millis(100), move || {
        match receiver.try_recv() {
            Ok(snapshot) => {
                info.set_text(&snapshot);
                gtk::glib::ControlFlow::Break
            }
            Err(mpsc::TryRecvError::Empty) => gtk::glib::ControlFlow::Continue,
            Err(mpsc::TryRecvError::Disconnected) => {
                info.set_text("Данные недоступны");
                gtk::glib::ControlFlow::Break
            }
        }
    });
    content
}

fn popup_content(kind: &str, config: &UiConfig) -> gtk::Widget {
    match kind {
        "calendar" => {
            let calendar = gtk::Calendar::new();
            calendar.upcast()
        }
        "launcher" => launcher_view().upcast(),
        "system" => system_view().upcast(),
        _ => {
            let box_view = gtk::Box::new(gtk::Orientation::Vertical, 12);
            let title = gtk::Label::new(Some("Настройки Kira"));
            title.add_css_class("section-title");
            box_view.append(&title);
            let detail = gtk::Label::new(Some(&format!(
                "Заголовок: {}\nКомпактная панель: {}\nСекунды в часах: {}\n\nРедактор дисплеев и сохранение KDL ещё не перенесены.",
                config.title, config.compact, config.show_seconds
            )));
            detail.set_wrap(true);
            detail.set_xalign(0.0);
            box_view.append(&detail);
            box_view.upcast()
        }
    }
}

fn popup_top_margin(kind: &str) -> i32 {
    if kind != "launcher" {
        return BAR_HEIGHT + POPUP_GAP;
    }
    let output_height = gtk::gdk::Display::default()
        .and_then(|display| display.monitors().item(0))
        .and_then(|item| item.downcast::<gtk::gdk::Monitor>().ok())
        .map(|monitor| monitor.geometry().height())
        .unwrap_or(800);
    (output_height / 5).max(BAR_HEIGHT + POPUP_GAP)
}

fn toggle_popup(
    app: &gtk::Application,
    kind: &str,
    popup: &Rc<RefCell<Option<(String, gtk::ApplicationWindow)>>>,
    config: &UiConfig,
) {
    // Keep the popup identity and lifetime independent of GTK widget contents.
    if let Some((previous_kind, previous_window)) = popup.borrow_mut().take() {
        previous_window.close();
        if previous_kind == kind {
            return;
        }
    }

    let window = gtk::ApplicationWindow::new(app);
    window.set_title(Some("Kira Desktop"));
    window.set_decorated(false);
    window.set_resizable(false);
    window.add_css_class("kira-popup");
    window.init_layer_shell();
    window.set_namespace(Some("kira-desktop-popup"));
    window.set_layer(Layer::Top);
    window.set_anchor(Edge::Top, true);
    window.set_exclusive_zone(0);
    window.set_keyboard_mode(KeyboardMode::OnDemand);
    window.set_margin(Edge::Top, popup_top_margin(kind));
    if kind == "launcher" {
        window.set_anchor(Edge::Left, false);
        window.set_anchor(Edge::Right, false);
    } else {
        window.set_anchor(Edge::Right, true);
        window.set_margin(Edge::Right, POPUP_GAP);
    }
    window.set_default_size(if kind == "settings" { 600 } else { 420 }, -1);

    let frame = gtk::Box::new(gtk::Orientation::Vertical, 14);
    frame.set_margin_top(18);
    frame.set_margin_bottom(18);
    frame.set_margin_start(18);
    frame.set_margin_end(18);
    frame.append(&popup_content(kind, config));
    window.set_child(Some(&frame));

    let weak_popup = Rc::downgrade(popup);
    window.connect_close_request(move |_| {
        if let Some(popup) = weak_popup.upgrade() {
            // The state may already have been taken by a toggle operation.
            if popup.try_borrow_mut().is_ok() {
                popup.borrow_mut().take();
            }
        }
        gtk::glib::Propagation::Proceed
    });

    popup
        .borrow_mut()
        .replace((kind.to_owned(), window.clone()));
    window.present();
}

fn start_core_subscriptions(sender: mpsc::Sender<CoreUpdate>) {
    let Ok(socket) = kira_ipc::core_socket_path() else {
        eprintln!("kira-shell: no XDG_RUNTIME_DIR, Niri state will remain offline");
        return;
    };
    let workspaces_sender = sender.clone();
    let workspaces_socket = socket.clone();
    std::thread::spawn(move || {
        loop {
            let result = kira_ipc::client::subscribe_workspaces(&workspaces_socket, |state| {
                let _ = workspaces_sender.send(CoreUpdate::Workspaces(state));
            });
            let _ = workspaces_sender.send(CoreUpdate::Workspaces(WorkspaceState::default()));
            if let Err(error) = result {
                eprintln!("kira-shell: workspace stream disconnected: {error}");
            }
            std::thread::sleep(Duration::from_millis(750));
        }
    });
    std::thread::spawn(move || {
        loop {
            if let Err(error) = kira_ipc::client::subscribe_config(&socket, |state| {
                let _ = sender.send(CoreUpdate::Config(state));
            }) {
                eprintln!("kira-shell: config stream disconnected: {error}");
            }
            std::thread::sleep(Duration::from_millis(750));
        }
    });
}

fn activate(app: &gtk::Application, primary: &Rc<RefCell<Option<gtk::ApplicationWindow>>>) {
    if let Some(window) = primary.borrow().as_ref() {
        window.present();
        return;
    }
    install_style();
    let bar = gtk::ApplicationWindow::new(app);
    bar.set_title(Some("Kira Desktop"));
    bar.set_decorated(false);
    bar.set_default_size(800, BAR_HEIGHT);
    bar.add_css_class("kira-bar");
    bar.init_layer_shell();
    bar.set_namespace(Some("kira-desktop"));
    bar.set_layer(Layer::Top);
    bar.set_anchor(Edge::Top, true);
    bar.set_anchor(Edge::Left, true);
    bar.set_anchor(Edge::Right, true);
    bar.set_exclusive_zone(BAR_HEIGHT);

    let layout = gtk::CenterBox::new();
    let workspace_box = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    workspace_box.set_margin_start(12);
    layout.set_start_widget(Some(&workspace_box));
    let title = gtk::Label::new(Some("KIRA DESKTOP"));
    title.add_css_class("bar-title");
    layout.set_center_widget(Some(&title));

    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 3);
    actions.set_margin_end(12);
    let clock = gtk::Label::new(Some("--:--"));
    clock.add_css_class("clock");
    let popup: Rc<RefCell<Option<(String, gtk::ApplicationWindow)>>> = Rc::new(RefCell::new(None));
    let config = Rc::new(RefCell::new(UiConfig::default()));
    for (kind, symbol, accessible) in [
        ("launcher", "⌕", "Запуск приложений"),
        ("calendar", "▦", "Календарь"),
        ("system", "◈", "Система"),
        ("settings", "⚙", "Настройки"),
    ] {
        let app = app.clone();
        let popup = popup.clone();
        let config = config.clone();
        actions.append(&make_button(symbol, accessible, move || {
            toggle_popup(&app, kind, &popup, &config.borrow());
        }));
    }
    actions.append(&clock);
    layout.set_end_widget(Some(&actions));
    bar.set_child(Some(&layout));

    let state = Rc::new(RefCell::new(WorkspaceState::default()));
    redraw_workspaces(&workspace_box, &state.borrow(), &config.borrow());

    let (sender, receiver) = mpsc::channel::<CoreUpdate>();
    start_core_subscriptions(sender);
    gtk::glib::timeout_add_local(Duration::from_millis(100), {
        let workspace_box = workspace_box.clone();
        let title = title.clone();
        let state = state.clone();
        let config = config.clone();
        move || {
            while let Ok(update) = receiver.try_recv() {
                match update {
                    CoreUpdate::Workspaces(next) => {
                        *state.borrow_mut() = next;
                        redraw_workspaces(&workspace_box, &state.borrow(), &config.borrow());
                    }
                    CoreUpdate::Config(next) => {
                        *config.borrow_mut() = next.config;
                        title.set_text(&config.borrow().title);
                        if next.error.is_some() {
                            title.set_tooltip_text(Some(
                                "Ошибка KDL. Сохранена предыдущая конфигурация.",
                            ));
                            title.add_css_class("config-error");
                        } else {
                            title.set_tooltip_text(None);
                            title.remove_css_class("config-error");
                        }
                        redraw_workspaces(&workspace_box, &state.borrow(), &config.borrow());
                    }
                }
            }
            gtk::glib::ControlFlow::Continue
        }
    });
    gtk::glib::timeout_add_local(Duration::from_secs(1), {
        let config = config.clone();
        move || {
            clock.set_text(&current_clock(config.borrow().show_seconds));
            gtk::glib::ControlFlow::Continue
        }
    });
    *primary.borrow_mut() = Some(bar.clone());
    bar.present();
}

fn main() {
    let flags = if std::env::var_os("KIRA_PREVIEW").is_some() {
        gtk::gio::ApplicationFlags::NON_UNIQUE
    } else {
        gtk::gio::ApplicationFlags::empty()
    };
    let app = gtk::Application::builder()
        .application_id("dev.kira.Desktop")
        .flags(flags)
        .build();
    let primary = Rc::new(RefCell::new(None));
    app.connect_activate(move |app| activate(app, &primary));
    app.run();
}

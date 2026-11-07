//! Desktop composition root: Core subscriptions and trusted services -> reusable kira-ui.
//! All GTK widget trees, CSS and transient window semantics are owned by kira-ui.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use gtk::prelude::*;
use kira_ipc::{ConfigState, PROTOCOL_VERSION, RpcRequest, UiConfig, WorkspaceState};
use kira_ui::{ApplicationEntry, Bar, InfoPanel, Launcher, PopupHost, PopupKind, WorkspaceItem};

enum ShellUpdate {
    Workspaces(WorkspaceState),
    Config(ConfigState),
    Applications(Vec<ApplicationEntry>),
    ApplicationLaunch(Result<(), String>),
    Metrics(String),
}

fn request_focus(id: String) {
    // Core validates this ID against its live snapshot before the fixed Niri action.
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

fn workspace_items(state: &WorkspaceState) -> Vec<WorkspaceItem> {
    state.workspaces.iter().map(|workspace| WorkspaceItem {
        id: workspace.id.clone(),
        index: workspace.idx,
        name: workspace.name.clone(),
        focused: workspace.is_focused,
        urgent: workspace.is_urgent,
    }).collect()
}

fn current_clock(show_seconds: bool) -> String {
    let format = if show_seconds { "%H:%M:%S" } else { "%H:%M" };
    gtk::glib::DateTime::now_local()
        .ok()
        .and_then(|time| time.format(format).ok())
        .map(|time| time.to_string())
        .unwrap_or_else(|| "--:--".to_owned())
}

fn settings_summary(config: &UiConfig) -> String {
    format!(
        "Заголовок: {}\nКомпактная панель: {}\nСекунды в часах: {}\n\nРедактор дисплеев и сохранение KDL ещё не перенесены.",
        config.title, config.compact, config.show_seconds
    )
}

/// Discover applications on a worker, converting GIO objects into Send-safe metadata.
fn start_application_discovery(sender: mpsc::Sender<ShellUpdate>) {
    std::thread::spawn(move || {
        let mut apps: Vec<ApplicationEntry> = gtk::gio::AppInfo::all()
            .into_iter()
            .filter(|app| app.should_show())
            .filter_map(|app| {
                app.id().map(|id| ApplicationEntry {
                    id: id.to_string(),
                    name: app.display_name().to_string(),
                })
            })
            .collect();
        apps.sort_by_key(|app| app.name.to_lowercase());
        apps.dedup_by(|a, b| a.id == b.id);
        let _ = sender.send(ShellUpdate::Applications(apps));
    });
}

fn launch_application(id: String, sender: mpsc::Sender<ShellUpdate>) {
    // Never construct a command from .desktop metadata or execute a shell.
    std::thread::spawn(move || {
        let result = gtk::gio::AppInfo::all()
            .into_iter()
            .find(|app| app.id().as_deref() == Some(id.as_str()))
            .ok_or_else(|| format!("Приложение больше не установлено: {id}"))
            .and_then(|app| {
                app.launch(&[], None::<&gtk::gio::AppLaunchContext>)
                    .map_err(|error| error.to_string())
            });
        let _ = sender.send(ShellUpdate::ApplicationLaunch(result));
    });
}

fn start_metrics(sender: mpsc::Sender<ShellUpdate>) {
    std::thread::spawn(move || {
        let mut sampler = kira_services::system_metrics::PulseSampler::default();
        loop {
            let pulse = sampler.sample();
            let cpu = pulse.cpu_percent
                .map(|value| format!("{value}%"))
                .unwrap_or_else(|| "недоступно".to_owned());
            let memory = pulse.memory_available_kib
                .map(|kib| format!("{:.1} GiB доступно", kib as f64 / 1024.0 / 1024.0))
                .unwrap_or_else(|| "недоступно".to_owned());
            let load = std::fs::read_to_string("/proc/loadavg")
                .ok()
                .map(|raw| raw.split_whitespace().take(3).collect::<Vec<_>>().join(" "))
                .unwrap_or_else(|| "недоступно".to_owned());
            if sender.send(ShellUpdate::Metrics(format!(
                "CPU: {cpu}\nОЗУ: {memory}\nНагрузка (1/5/15 мин): {load}"
            ))).is_err() {
                return;
            }
            std::thread::sleep(Duration::from_secs(2));
        }
    });
}

fn start_core_subscriptions(sender: mpsc::Sender<ShellUpdate>) {
    let Ok(socket) = kira_ipc::core_socket_path() else {
        eprintln!("kira-shell: no XDG_RUNTIME_DIR, Niri state will remain offline");
        return;
    };
    let workspaces_sender = sender.clone();
    let workspaces_socket = socket.clone();
    std::thread::spawn(move || loop {
        let result = kira_ipc::client::subscribe_workspaces(&workspaces_socket, |state| {
            let _ = workspaces_sender.send(ShellUpdate::Workspaces(state));
        });
        let _ = workspaces_sender.send(ShellUpdate::Workspaces(WorkspaceState::default()));
        if let Err(error) = result {
            eprintln!("kira-shell: workspace stream disconnected: {error}");
        }
        std::thread::sleep(Duration::from_millis(750));
    });
    std::thread::spawn(move || loop {
        if let Err(error) = kira_ipc::client::subscribe_config(&socket, |state| {
            let _ = sender.send(ShellUpdate::Config(state));
        }) {
            eprintln!("kira-shell: config stream disconnected: {error}");
        }
        std::thread::sleep(Duration::from_millis(750));
    });
}

fn activate(app: &gtk::Application, primary: &Rc<RefCell<Option<gtk::ApplicationWindow>>>) {
    if let Some(window) = primary.borrow().as_ref() {
        window.present();
        return;
    }
    kira_ui::install_style();
    let bar = Rc::new(Bar::new(app, request_focus));
    let popup = Rc::new(PopupHost::new(app));
    let config = Rc::new(RefCell::new(UiConfig::default()));
    let state = Rc::new(RefCell::new(WorkspaceState::default()));
    let applications = Rc::new(RefCell::new(Vec::<ApplicationEntry>::new()));
    let launcher: Rc<RefCell<Option<Launcher>>> = Rc::new(RefCell::new(None));
    let metrics = Rc::new(InfoPanel::new("Система", "Загрузка данных"));
    let settings = Rc::new(InfoPanel::new("Настройки Kira", &settings_summary(&config.borrow())));

    let (sender, receiver) = mpsc::channel::<ShellUpdate>();
    start_core_subscriptions(sender.clone());
    start_application_discovery(sender.clone());
    start_metrics(sender.clone());

    for (kind, symbol, label) in [
        (PopupKind::Launcher, "⌕", "Запуск приложений"),
        (PopupKind::Calendar, "▦", "Календарь"),
        (PopupKind::System, "◈", "Система"),
        (PopupKind::Settings, "⚙", "Настройки"),
    ] {
        let popup = popup.clone();
        let launcher = launcher.clone();
        let applications = applications.clone();
        let metrics = metrics.clone();
        let settings = settings.clone();
        let sender = sender.clone();
        bar.add_action(symbol, label, move || {
            popup.toggle(kind, || match kind {
                PopupKind::Launcher => {
                    let mut handle = launcher.borrow_mut();
                    handle.get_or_insert_with(|| {
                        let sender = sender.clone();
                        let view = Launcher::new(move |id| launch_application(id, sender.clone()));
                        if !applications.borrow().is_empty() {
                            view.set_applications(applications.borrow().clone());
                        }
                        view
                    }).widget()
                }
                PopupKind::Calendar => kira_ui::calendar_view(),
                PopupKind::System => metrics.widget(),
                PopupKind::Settings => settings.widget(),
            });
        });
    }
    bar.finish_actions();

    bar.workspaces.update(false, &[], usize::from(config.borrow().max_workspaces), config.borrow().show_names);

    gtk::glib::timeout_add_local(Duration::from_millis(100), {
        let bar = bar.clone();
        let config = config.clone();
        let state = state.clone();
        let applications = applications.clone();
        let launcher = launcher.clone();
        let metrics = metrics.clone();
        let settings = settings.clone();
        move || {
            // Avoid holding the GTK main thread indefinitely during event bursts.
            for update in receiver.try_iter().take(64) {
                match update {
                    ShellUpdate::Workspaces(next) => {
                        *state.borrow_mut() = next;
                        let settings = config.borrow();
                        bar.workspaces.update(
                            state.borrow().connected,
                            &workspace_items(&state.borrow()),
                            usize::from(settings.max_workspaces),
                            settings.show_names,
                        );
                    }
                    ShellUpdate::Config(next) => {
                        *config.borrow_mut() = next.config;
                        let current = config.borrow();
                        bar.title.set_text(&current.title);
                        bar.set_compact(current.compact);
                        settings.set_details(&settings_summary(&current));
                        if next.error.is_some() {
                            bar.title.set_tooltip_text(Some("Ошибка KDL. Сохранена предыдущая конфигурация."));
                            bar.title.add_css_class("config-error");
                        } else {
                            bar.title.set_tooltip_text(None);
                            bar.title.remove_css_class("config-error");
                        }
                        bar.workspaces.update(
                            state.borrow().connected,
                            &workspace_items(&state.borrow()),
                            usize::from(current.max_workspaces),
                            current.show_names,
                        );
                    }
                    ShellUpdate::Applications(next) => {
                        *applications.borrow_mut() = next.clone();
                        if let Some(view) = launcher.borrow().as_ref() {
                            view.set_applications(next);
                        }
                    }
                    ShellUpdate::ApplicationLaunch(result) => {
                        if let Some(view) = launcher.borrow().as_ref() {
                            view.set_status(match result {
                                Ok(()) => "Приложение запущено",
                                Err(ref error) => error,
                            });
                        }
                    }
                    ShellUpdate::Metrics(snapshot) => metrics.set_details(&snapshot),
                }
            }
            gtk::glib::ControlFlow::Continue
        }
    });
    bar.clock.set_text(&current_clock(config.borrow().show_seconds));
    gtk::glib::timeout_add_local(Duration::from_secs(1), {
        let config = config.clone();
        let clock = bar.clock.clone();
        move || {
            clock.set_text(&current_clock(config.borrow().show_seconds));
            gtk::glib::ControlFlow::Continue
        }
    });

    *primary.borrow_mut() = Some(bar.window.clone());
    bar.window.present();
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

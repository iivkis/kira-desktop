//! Desktop composition root: Core subscriptions and trusted services -> reusable kira-ui.
//! All GTK widget trees, CSS and transient window semantics are owned by kira-ui.

mod display_bridge;

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use crate::display_bridge::decode_outputs;
use gtk::prelude::*;
use kira_ipc::{ConfigState, PROTOCOL_VERSION, RpcRequest, UiConfig, WorkspaceState};
use kira_services::display_manager::{Event as DisplayEvent, Model as DisplayModel};
use kira_ui::{
    ApplicationEntry, Bar, DisplayIntent, DisplayPhase, DisplaySettings, GpuReading, InfoPanel,
    Launcher, PopupHost, PopupKind, ProcessItem, ProcessList, SystemMonitor, SystemOverview,
    WorkspaceItem,
};

enum ShellUpdate {
    Workspaces(WorkspaceState),
    Config(ConfigState),
    Applications(Vec<ApplicationEntry>),
    ApplicationLaunch(Result<(), String>),
    SystemOverview(SystemOverview),
    Processes(ProcessList),
    ProcessError(String),
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
    state
        .workspaces
        .iter()
        .map(|workspace| WorkspaceItem {
            id: workspace.id.clone(),
            index: workspace.idx,
            name: workspace.name.clone(),
            focused: workspace.is_focused,
            urgent: workspace.is_urgent,
        })
        .collect()
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
        "Заголовок: {}\nКомпактная панель: {}\nСекунды в часах: {}\n\nРедактор настроек Kira KDL ещё не перенесён. Настройка дисплеев находится ниже.",
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

/// Heavy Linux telemetry and /proc process scans never run on the GTK thread.
/// Processes are enumerated only while their tab is visible to the user.
fn start_system_sampler(
    sender: mpsc::Sender<ShellUpdate>,
    system_visible: Arc<AtomicBool>,
    processes_visible: Arc<AtomicBool>,
) {
    std::thread::spawn(move || {
        let mut metrics = kira_services::system_metrics::Sampler::default();
        let mut processes = kira_services::process_monitor::ProcessSampler::default();
        loop {
            if system_visible.load(Ordering::Relaxed) {
                let snapshot = metrics.sample();
                let overview = SystemOverview {
                    cpu_percent: snapshot.cpu_percent,
                    cpu_temperature_c: snapshot.cpu_temperature_c,
                    memory_total_kib: snapshot.memory_total_kib,
                    memory_available_kib: snapshot.memory_available_kib,
                    load: snapshot.load,
                    uptime_seconds: snapshot.uptime_seconds,
                    battery_percent: snapshot.battery_percent,
                    battery_charging: snapshot.battery_charging,
                    gpu: snapshot
                        .gpus
                        .into_iter()
                        .map(|gpu| GpuReading {
                            name: gpu.label,
                            utilization_percent: gpu.utilization_percent,
                            temperature_c: gpu.temperature_c,
                        })
                        .collect(),
                };
                if sender.send(ShellUpdate::SystemOverview(overview)).is_err() {
                    return;
                }
                if processes_visible.load(Ordering::Relaxed) {
                    let event = match processes.sample() {
                        Ok(snapshot) => ShellUpdate::Processes(ProcessList {
                            rows: snapshot
                                .rows
                                .into_iter()
                                .map(|process| ProcessItem {
                                    pid: process.pid,
                                    name: process.name,
                                    cpu_percent: process.cpu_percent,
                                    rss_kib: process.rss_kib,
                                    pss_kib: process.pss_kib,
                                })
                                .collect(),
                            total: snapshot.total,
                            truncated: snapshot.truncated,
                        }),
                        Err(error) => ShellUpdate::ProcessError(format!(
                            "Не удалось прочитать процессы: {error}"
                        )),
                    };
                    if sender.send(event).is_err() {
                        return;
                    }
                }
            }
            std::thread::sleep(Duration::from_secs(2));
        }
    });
}

fn display_mutations_enabled() -> bool {
    // The 15s watchdog currently lives in the shell process. Until it moves
    // into Core, never mutate the live user's outputs from the normal desktop.
    // This flag is a migration safety interlock, not a privilege boundary.
    std::env::var_os("KIRA_PREVIEW").is_some()
}

fn send_display_intent(model: &DisplayModel, intent: DisplayIntent) {
    if !display_mutations_enabled()
        && matches!(
            &intent,
            DisplayIntent::Apply(_) | DisplayIntent::Confirm(_) | DisplayIntent::Save
        )
    {
        eprintln!("kira-shell: live display mutation is disabled until Core owns rollback");
        return;
    }
    match intent {
        DisplayIntent::Refresh => model.refresh(),
        DisplayIntent::Apply(choices) => {
            let request = choices
                .into_iter()
                .map(|output| {
                    serde_json::json!({
                        "connector": output.connector,
                        "enabled": output.enabled,
                        "mode": output.mode,
                        "scale": output.scale,
                        "transform": output.transform,
                        "x": output.x,
                        "y": output.y,
                        "vrr": output.vrr,
                    })
                })
                .collect::<Vec<_>>();
            model.apply(serde_json::Value::Array(request));
        }
        DisplayIntent::Confirm(id) => model.confirm(id),
        DisplayIntent::Revert(id) => model.revert(id),
        DisplayIntent::Save => model.save(),
    }
}

fn handle_display_event(display: &DisplaySettings, event: DisplayEvent) {
    display.set_persistence(event.persistence);
    if let Some(error) = event.error {
        display.set_phase(DisplayPhase::Error(error));
        return;
    }
    match event.state {
        "ready" | "reverted" | "saved" => {
            let Some(snapshot) = event.outputs else {
                display.set_phase(DisplayPhase::Error(
                    "Niri не вернул состояние дисплеев".into(),
                ));
                return;
            };
            match decode_outputs(&snapshot) {
                Ok(outputs) => {
                    display.set_outputs(outputs);
                    display.set_phase(match event.state {
                        "saved" => DisplayPhase::Saved,
                        "reverted" => DisplayPhase::Reverted,
                        _ => DisplayPhase::Ready,
                    });
                }
                Err(error) => display.set_phase(DisplayPhase::Error(error)),
            }
        }
        "pending" => match (event.pending_id, event.deadline_unix_ms) {
            (Some(id), Some(deadline_unix_ms)) => {
                display.set_phase(DisplayPhase::Pending {
                    id,
                    deadline_unix_ms,
                });
            }
            _ => display.set_phase(DisplayPhase::Error(
                "Нет данных для подтверждения дисплеев".into(),
            )),
        },
        "confirmed" => display.set_phase(DisplayPhase::Confirmed),
        "error" => display.set_phase(DisplayPhase::Error("Ошибка настроек дисплеев".into())),
        _ => display.set_phase(DisplayPhase::Error("Неизвестное состояние дисплеев".into())),
    }
}

fn start_core_subscriptions(sender: mpsc::Sender<ShellUpdate>) {
    let Ok(socket) = kira_ipc::core_socket_path() else {
        eprintln!("kira-shell: no XDG_RUNTIME_DIR, Niri state will remain offline");
        return;
    };
    let workspaces_sender = sender.clone();
    let workspaces_socket = socket.clone();
    std::thread::spawn(move || {
        loop {
            let result = kira_ipc::client::subscribe_workspaces(&workspaces_socket, |state| {
                let _ = workspaces_sender.send(ShellUpdate::Workspaces(state));
            });
            let _ = workspaces_sender.send(ShellUpdate::Workspaces(WorkspaceState::default()));
            if let Err(error) = result {
                eprintln!("kira-shell: workspace stream disconnected: {error}");
            }
            std::thread::sleep(Duration::from_millis(750));
        }
    });
    std::thread::spawn(move || {
        loop {
            if let Err(error) = kira_ipc::client::subscribe_config(&socket, |state| {
                let _ = sender.send(ShellUpdate::Config(state));
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
    kira_ui::install_style();
    let bar = Rc::new(Bar::new(app, request_focus));
    let popup = Rc::new(PopupHost::new(app));
    let config = Rc::new(RefCell::new(UiConfig::default()));
    let state = Rc::new(RefCell::new(WorkspaceState::default()));
    let applications = Rc::new(RefCell::new(Vec::<ApplicationEntry>::new()));
    let launcher: Rc<RefCell<Option<Launcher>>> = Rc::new(RefCell::new(None));
    let system = Rc::new(SystemMonitor::new());
    let settings = Rc::new(InfoPanel::new(
        "Настройки Kira",
        &settings_summary(&config.borrow()),
    ));
    let display_model = Rc::new(DisplayModel::new());
    let displays = Rc::new(DisplaySettings::new({
        let model = display_model.clone();
        move |intent| send_display_intent(&model, intent)
    }));
    displays.set_editing_enabled(display_mutations_enabled());
    let settings_content = kira_ui::settings_page(&settings.widget(), &displays.widget());

    let (sender, receiver) = mpsc::channel::<ShellUpdate>();
    let system_visible = Arc::new(AtomicBool::new(false));
    let processes_visible = Arc::new(AtomicBool::new(false));
    start_core_subscriptions(sender.clone());
    start_application_discovery(sender.clone());
    start_system_sampler(
        sender.clone(),
        system_visible.clone(),
        processes_visible.clone(),
    );

    for (kind, symbol, label) in [
        (PopupKind::Launcher, "⌕", "Запуск приложений"),
        (PopupKind::Calendar, "▦", "Календарь"),
        (PopupKind::System, "◈", "Система"),
        (PopupKind::Settings, "⚙", "Настройки"),
    ] {
        let popup = popup.clone();
        let launcher = launcher.clone();
        let applications = applications.clone();
        let system = system.clone();
        let settings_content = settings_content.clone();
        let displays = displays.clone();
        let display_model = display_model.clone();
        let sender = sender.clone();
        bar.add_action(symbol, label, move || {
            popup.toggle(kind, || match kind {
                PopupKind::Launcher => {
                    let mut handle = launcher.borrow_mut();
                    handle
                        .get_or_insert_with(|| {
                            let sender = sender.clone();
                            let view =
                                Launcher::new(move |id| launch_application(id, sender.clone()));
                            if !applications.borrow().is_empty() {
                                view.set_applications(applications.borrow().clone());
                            }
                            view
                        })
                        .widget()
                }
                PopupKind::Calendar => kira_ui::calendar_view(),
                PopupKind::System => system.widget(),
                PopupKind::Settings => settings_content.clone(),
            });
            if kind == PopupKind::Settings && popup.is_active(PopupKind::Settings) {
                // Keep an explicit Confirm/Save transaction alive across view switches.
                if !matches!(
                    displays.phase(),
                    DisplayPhase::Pending { .. }
                        | DisplayPhase::Confirmed
                        | DisplayPhase::Applying
                        | DisplayPhase::Confirming
                        | DisplayPhase::Saving
                        | DisplayPhase::Reverting
                ) {
                    displays.set_phase(DisplayPhase::Loading);
                    display_model.refresh();
                }
            }
        });
    }
    bar.finish_actions();

    bar.workspaces.update(
        false,
        &[],
        usize::from(config.borrow().max_workspaces),
        config.borrow().show_names,
    );

    gtk::glib::timeout_add_local(Duration::from_millis(100), {
        let bar = bar.clone();
        let config = config.clone();
        let state = state.clone();
        let applications = applications.clone();
        let launcher = launcher.clone();
        let system = system.clone();
        let settings = settings.clone();
        let popup = popup.clone();
        let system_visible = system_visible.clone();
        let processes_visible = processes_visible.clone();
        let display_model = display_model.clone();
        let displays = displays.clone();
        move || {
            let active_system = popup.is_active(PopupKind::System);
            system_visible.store(active_system, Ordering::Relaxed);
            processes_visible.store(
                active_system && system.processes_selected(),
                Ordering::Relaxed,
            );
            // Hiding an unconfirmed Settings popup (including Escape) asks the
            // trusted Niri worker to rollback immediately; its watchdog remains
            // authoritative if the request is delayed or the shell crashes.
            if !popup.is_active(PopupKind::Settings)
                && let Some(id) = displays.pending_id()
            {
                displays.set_phase(DisplayPhase::Reverting);
                display_model.revert(id);
            }
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
                            bar.title.set_tooltip_text(Some(
                                "Ошибка KDL. Сохранена предыдущая конфигурация.",
                            ));
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
                    ShellUpdate::SystemOverview(snapshot) => system.set_overview(&snapshot),
                    ShellUpdate::Processes(snapshot) => system.set_processes(&snapshot),
                    ShellUpdate::ProcessError(error) => system.set_process_error(&error),
                }
            }
            for event in display_model.poll() {
                handle_display_event(&displays, event);
            }
            gtk::glib::ControlFlow::Continue
        }
    });
    bar.clock
        .set_text(&current_clock(config.borrow().show_seconds));
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

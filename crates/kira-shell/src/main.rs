//! GTK4/WebKitGTK host and bounded JSON-RPC bridge to the Core daemon.

mod launcher;
mod ui_assets;
use std::env;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use gtk::prelude::*;
use gtk4_layer_shell::{Edge, Layer, LayerShell};
use kira_ipc::{
    ConfigState, DesktopApplication, PROTOCOL_VERSION, RpcRequest, RpcResponse, WorkspaceState,
};
use webkit6::prelude::*;

const BAR_HEIGHT: i32 = 42;
const MAX_BRIDGE_MESSAGE_BYTES: usize = 4096;

/// Only the GTK main loop dispatches JS events; the worker threads contain
/// no GTK/WebKit handles.
enum ShellDelivery {
    Rpc(String),
    Workspaces(WorkspaceState),
    Config(ConfigState),
    LauncherToggle,
    LauncherCatalog(Result<Vec<DesktopApplication>, String>),
    LauncherLaunched(Result<(), String>),
}

fn ui_index() -> PathBuf {
    let directory = env::var_os("KIRA_UI_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../frontend/dist"));
    directory.join("index.html")
}

// Navigation is limited to the exact packaged UI document. Prefix matching
// would allow unrelated file:// paths (or attacker-controlled remote pages).
fn permitted_ui_page(requested_uri: &str, index_uri: &str) -> bool {
    requested_uri == index_uri
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

    // The build embeds CSS and JS in index.html. Load it directly as an
    // about:blank document so WebKit never needs to execute scripts from a
    // custom URI origin. Keep the previous scheme for opt-in diagnostics.
    let html = std::fs::read_to_string(&index).unwrap_or_else(|error| {
        eprintln!(
            "kira-shell: failed to read UI HTML {}: {error}",
            index.display()
        );
        std::process::exit(1);
    });
    let use_legacy_scheme = env::var_os("KIRA_WEBKIT_LEGACY_SCHEME").is_some();

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

        // Vite's production JS is an ES module. The file:// scheme blocks
        // module requests in WebKitGTK; serve packaged files as a private
        // CORS-enabled scheme instead of granting file:// broad access.
        let context = webkit6::WebContext::new();
        if let Some(security) = context.security_manager() {
            security.register_uri_scheme_as_local("kira-ui");
            security.register_uri_scheme_as_secure("kira-ui");
            security.register_uri_scheme_as_cors_enabled("kira-ui");
            security.register_uri_scheme_as_display_isolated("kira-ui");
        }

        let ui_root = index.parent().expect("UI index must have a parent").to_path_buf();
        context.register_uri_scheme("kira-ui", move |request| {
            let Some(uri) = request.uri() else {
                let mut error = gtk::glib::Error::new(
                    gtk::gio::IOErrorEnum::InvalidArgument,
                    "Missing Kira UI asset URI",
                );
                request.finish_error(&mut error);
                return;
            };
            match ui_assets::read_asset(uri.as_str(), &ui_root) {
                Ok((content, mime)) => {
                    let length = content.len() as i64;
                    let bytes = gtk::glib::Bytes::from_owned(content);
                    let stream = gtk::gio::MemoryInputStream::from_bytes(&bytes);
                    request.finish(&stream, length, Some(mime));
                }
                Err(error) => {
                    eprintln!("kira-shell: UI asset denied: {uri}: {error}");
                    let mut webkit_error = gtk::glib::Error::new(
                        gtk::gio::IOErrorEnum::NotFound,
                        "Kira UI asset not found",
                    );
                    request.finish_error(&mut webkit_error);
                }
            }
        });

        let manager = webkit6::UserContentManager::new();
        let view = webkit6::WebView::builder()
            .web_context(&context)
            .user_content_manager(&manager)
            .build();
        view.set_background_color(&gtk::gdk::RGBA::new(0.09, 0.09, 0.12, 1.0));

        // GTK and WebKit are not Send: socket IO runs on workers, UI updates
        // return to the GTK main loop through this channel.
        let (sender, receiver) = mpsc::channel::<ShellDelivery>();
        let socket = core_socket();

        // One long-lived, read-only Core subscription. Core receives Niri
        // EventStream and immediately forwards changed snapshots to this
        // worker, avoiding the previous 1.5-second Svelte polling lag.
        if let Some(path) = socket.clone() {
            let updates = sender.clone();
            std::thread::spawn(move || {
                loop {
                    let result = kira_ipc::client::subscribe_workspaces(&path, |state| {
                        let _ = updates.send(ShellDelivery::Workspaces(state));
                    });
                    // Never show stale workspace data after a Core failure.
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
        let launcher_sender = sender.clone();
        let launcher_socket = socket.clone();
        manager.connect_script_message_received(Some("kira"), move |_, value| {
            let message = value.to_string().to_string();
            let id = serde_json::from_str::<RpcRequest>(&message).ok().map(|r| r.id);
            let sender = sender.clone();
            let socket = socket.clone();
            // Only a narrow UI toggle is processed by the host. All filesystem
            // reads and launches remain Core operations, never JS-provided Exec.
            if message.len() <= MAX_BRIDGE_MESSAGE_BYTES {
                if let Ok(request) = serde_json::from_str::<RpcRequest>(&message) {
                    if request.jsonrpc == PROTOCOL_VERSION
                        && request.method == "toggle_launcher"
                        && request.params.is_none()
                    {
                        let _ = sender.send(ShellDelivery::LauncherToggle);
                        let reply = RpcResponse::success(request.id, serde_json::json!({"accepted": true}));
                        if let Ok(serialized) = serde_json::to_string(&reply) {
                            let _ = sender.send(ShellDelivery::Rpc(serialized));
                        }
                        return;
                    }
                }
            }
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

        view.connect_load_failed(|_, event, uri, error| {
            eprintln!("kira-shell: WebKit load failed ({event:?}) for {uri}: {error}");
            false
        });
        view.connect_web_process_terminated(|_, reason| {
            eprintln!("kira-shell: WebKit renderer terminated: {reason:?}");
        });
        view.connect_load_changed(|webview, event| {
            if event == webkit6::LoadEvent::Finished {
                let webview = webview.clone();
                gtk::glib::timeout_add_local_once(Duration::from_millis(1500), move || {
                    webview.evaluate_javascript(
                        "if (!document.querySelector('main.bar')) { throw new Error('Svelte UI did not mount; ' + 'boot=' + JSON.stringify(window.__kiraBoot || {}) + '; readyState=' + document.readyState + '; scripts=' + document.scripts.length); }",
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

        let launcher_ui = launcher::create(app, &view, launcher_socket.clone(), launcher_sender.clone());
        let webview = view.clone();
        gtk::glib::timeout_add_local(Duration::from_millis(30), move || {
            while let Ok(delivery) = receiver.try_recv() {
                // Double encoding ensures payload content cannot become JS.
                let (event, payload) = match delivery {
                    ShellDelivery::LauncherToggle => {
                        if launcher_ui.window.is_visible() {
                            launcher_ui.window.hide();
                            webview.grab_focus();
                        } else {
                            launcher_ui.show_loading();
                            launcher::open_catalog(launcher_socket.clone(), launcher_sender.clone());
                        }
                        continue;
                    }
                    ShellDelivery::LauncherCatalog(result) => {
                        launcher_ui.set_catalog(result);
                        continue;
                    }
                    ShellDelivery::LauncherLaunched(result) => {
                        launcher_ui.launch_finished(result, &webview);
                        continue;
                    }
                    ShellDelivery::Rpc(serialized) => ("kira:rpc", serialized),
                    ShellDelivery::Workspaces(state) => {
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

        let allowed_uri = if use_legacy_scheme {
            ui_assets::UI_DOCUMENT_URI.to_string()
        } else {
            "about:blank".to_string()
        };
        view.connect_decide_policy(move |_, decision, kind| {
            if kind == webkit6::PolicyDecisionType::NewWindowAction {
                decision.ignore();
                return true;
            }
            if kind == webkit6::PolicyDecisionType::NavigationAction {
                let permitted = decision
                    .downcast_ref::<webkit6::NavigationPolicyDecision>()
                    .and_then(|navigation| navigation.navigation_action())
                    .and_then(|action| action.request())
                    .and_then(|request| request.uri())
                    .is_some_and(|uri| permitted_ui_page(uri.as_str(), &allowed_uri));
                if !permitted {
                    decision.ignore();
                    return true;
                }
            }
            false
        });
        if use_legacy_scheme {
            eprintln!("kira-shell: diagnostic legacy scheme {}", ui_assets::UI_DOCUMENT_URI);
            view.load_uri(ui_assets::UI_DOCUMENT_URI);
        } else {
            eprintln!("kira-shell: loading self-contained HTML using WebKit load_html()");
            view.load_html(&html, None);
        }
        window.set_child(Some(&view));
        window.present();
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
        for local in [ui_assets::UI_DOCUMENT_URI, "about:blank"] {
            assert!(permitted_ui_page(local, local));
            assert!(!permitted_ui_page("https://evil.example/", local));
            assert!(!permitted_ui_page("kira-ui://app/index.html.evil", local));
            assert!(!permitted_ui_page("file:///etc/passwd", local));
        }
    }

    #[test]
    fn browser_bridge_cannot_run_commands_or_activate_apps_directly() {
        for method in [
            "launch_application",
            "execute",
            "get_applications",
            "gio_launch",
        ] {
            let request = format!(
                r#"{{"jsonrpc":"2.0","id":11,"method":"{method}","params":{{"id":"foo.desktop"}}}}"#
            );
            assert_eq!(process_message(&request, None).error.unwrap().code, -32601);
        }
    }

    #[test]
    fn reports_missing_core() {
        let result = process_message(r#"{"jsonrpc":"2.0","id":3,"method":"ping"}"#, None);
        assert_eq!(result.error.unwrap().code, -32000);
    }
}

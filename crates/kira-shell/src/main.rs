//! GTK4/WebKitGTK host and bounded JSON-RPC bridge to the Core daemon.
use std::env;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use gtk::prelude::*;
use gtk4_layer_shell::{Edge, Layer, LayerShell};
use kira_ipc::{PROTOCOL_VERSION, RpcRequest, RpcResponse};
use webkit6::prelude::*;

const BAR_HEIGHT: i32 = 42;
const MAX_BRIDGE_MESSAGE_BYTES: usize = 4096;

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

/// Only expose explicit, read-only Core operations to the Svelte page.
fn process_message(message: &str, socket: Option<&Path>) -> RpcResponse {
    let request = match serde_json::from_str::<RpcRequest>(message) {
        Ok(value) => value,
        Err(_) => return RpcResponse::failure(None, -32700, "Invalid JSON-RPC message"),
    };
    if request.jsonrpc != PROTOCOL_VERSION {
        return RpcResponse::failure(Some(request.id), -32600, "Invalid JSON-RPC version");
    }
    if !matches!(request.method.as_str(), "ping" | "get_state") {
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

        let manager = webkit6::UserContentManager::new();
        let view = webkit6::WebView::builder()
            .user_content_manager(&manager)
            .build();
        view.set_background_color(&gtk::gdk::RGBA::new(0.09, 0.09, 0.12, 1.0));

        // GTK and WebKit are not Send: socket IO runs on workers, UI updates
        // return to the GTK main loop through this channel.
        let (sender, receiver) = mpsc::channel::<String>();
        let socket = core_socket();
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
                    let _ = sender.send(serialized);
                }
            });
        });
        assert!(manager.register_script_message_handler("kira", None));

        let webview = view.clone();
        gtk::glib::timeout_add_local(Duration::from_millis(30), move || {
            while let Ok(response) = receiver.try_recv() {
                // Double encoding ensures response content cannot become JS source.
                if let Ok(encoded) = serde_json::to_string(&response) {
                    let script = format!(
                        "window.dispatchEvent(new CustomEvent('kira:rpc', {{ detail: JSON.parse({encoded}) }}));"
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

        let file_uri = gtk::gio::File::for_path(&index).uri();
        let allowed_uri = file_uri.to_string();
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
        view.load_uri(file_uri.as_str());
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
    fn navigation_is_restricted_to_exact_ui_document() {
        let local = "file:///nix/store/kira/ui/index.html";
        assert!(permitted_ui_page(local, local));
        assert!(!permitted_ui_page("https://evil.example/", local));
        assert!(!permitted_ui_page(
            "file:///nix/store/kira/ui/index.html.evil",
            local
        ));
        assert!(!permitted_ui_page("file:///etc/passwd", local));
    }

    #[test]
    fn reports_missing_core() {
        let result = process_message(r#"{"jsonrpc":"2.0","id":3,"method":"ping"}"#, None);
        assert_eq!(result.error.unwrap().code, -32000);
    }
}

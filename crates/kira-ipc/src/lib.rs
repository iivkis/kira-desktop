//! Read-only, bounded JSON-RPC state shared between Core and the GTK Shell.
//! Niri EventStream updates are owned by Core; UI can only query snapshots.

pub mod client;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const PROTOCOL_VERSION: &str = "2.0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RpcRequest {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RpcResponse {
    pub jsonrpc: String,
    pub id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
}

/// Niri workspace id is a u64. Send it as a string to avoid JS precision loss.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workspace {
    pub id: String,
    pub idx: u8,
    pub name: Option<String>,
    pub output: Option<String>,
    pub is_active: bool,
    pub is_focused: bool,
    pub is_urgent: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceState {
    pub connected: bool,
    pub workspaces: Vec<Workspace>,
}

/// Validated KDL appearance settings sent to the trusted frontend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UiConfig {
    pub compact: bool,
    pub title: String,
    pub show_seconds: bool,
    pub show_names: bool,
    pub max_workspaces: u8,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            compact: false,
            title: "KIRA DESKTOP".into(),
            show_seconds: false,
            show_names: true,
            max_workspaces: 12,
        }
    }
}

/// Last successfully parsed config plus an optional validation diagnostic.
/// Invalid reloads preserve the previous working UiConfig.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ConfigState {
    pub config: UiConfig,
    pub error: Option<String>,
}

impl RpcResponse {
    pub fn success(id: u64, value: Value) -> Self {
        Self {
            jsonrpc: PROTOCOL_VERSION.into(),
            id: Some(id),
            result: Some(value),
            error: None,
        }
    }

    pub fn failure(id: Option<u64>, code: i32, message: &str) -> Self {
        Self {
            jsonrpc: PROTOCOL_VERSION.into(),
            id,
            result: None,
            error: Some(RpcError {
                code,
                message: message.into(),
            }),
        }
    }
}

pub fn dispatch(request: RpcRequest) -> RpcResponse {
    dispatch_with_workspaces(request, &WorkspaceState::default())
}

pub fn dispatch_with_workspaces(request: RpcRequest, state: &WorkspaceState) -> RpcResponse {
    dispatch_with_state(request, state, &ConfigState::default())
}

pub fn dispatch_with_state(
    request: RpcRequest,
    workspaces: &WorkspaceState,
    config: &ConfigState,
) -> RpcResponse {
    if request.jsonrpc != PROTOCOL_VERSION {
        return RpcResponse::failure(Some(request.id), -32600, "Invalid Request");
    }

    match request.method.as_str() {
        "ping" => RpcResponse::success(request.id, json!("pong")),
        "get_state" => RpcResponse::success(
            request.id,
            json!({ "version": env!("CARGO_PKG_VERSION"), "status": "bootstrap" }),
        ),
        "get_workspaces" => RpcResponse::success(request.id, json!(workspaces)),
        "get_config" => RpcResponse::success(request.id, json!(config)),
        _ => RpcResponse::failure(Some(request.id), -32601, "Method not found"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_round_trip() {
        let request: RpcRequest =
            serde_json::from_str(r#"{"jsonrpc":"2.0","id":7,"method":"ping"}"#).unwrap();
        let response = dispatch(request);
        assert_eq!(response.result, Some(json!("pong")));
        assert_eq!(response.id, Some(7));
        assert_eq!(response.error, None);
        assert!(
            serde_json::to_string(&response)
                .unwrap()
                .contains("\"result\":\"pong\"")
        );
    }

    #[test]
    fn invalid_method_has_json_rpc_error_code() {
        let response = dispatch(RpcRequest {
            jsonrpc: PROTOCOL_VERSION.into(),
            id: 1,
            method: "missing".into(),
        });
        assert_eq!(response.error.unwrap().code, -32601);
    }

    #[test]
    fn invalid_version_is_rejected() {
        let response = dispatch(RpcRequest {
            jsonrpc: "1.0".into(),
            id: 2,
            method: "ping".into(),
        });
        assert_eq!(response.error.unwrap().code, -32600);
    }

    #[test]
    fn default_config_is_read_only_and_typed() {
        let req = RpcRequest {
            jsonrpc: PROTOCOL_VERSION.into(),
            id: 13,
            method: "get_config".into(),
        };
        let response = dispatch(req);
        assert_eq!(
            response.result.unwrap(),
            json!({
                "config": {"compact": false, "title": "KIRA DESKTOP",
                  "show_seconds": false, "show_names": true, "max_workspaces": 12},
                "error": null
            })
        );
        assert_eq!(
            dispatch(RpcRequest {
                jsonrpc: PROTOCOL_VERSION.into(),
                id: 14,
                method: "set_config".into()
            })
            .error
            .unwrap()
            .code,
            -32601
        );
    }

    #[test]
    fn disconnected_niri_does_not_fabricate_workspaces() {
        let response = dispatch(RpcRequest {
            jsonrpc: PROTOCOL_VERSION.into(),
            id: 3,
            method: "get_workspaces".into(),
        });
        assert_eq!(
            response.result,
            Some(json!({"connected": false, "workspaces": []}))
        );
    }

    #[test]
    fn typed_workspaces_preserve_large_ids_and_flags() {
        let state = WorkspaceState {
            connected: true,
            workspaces: vec![Workspace {
                id: u64::MAX.to_string(),
                idx: 2,
                name: Some("code".into()),
                output: Some("DP-1".into()),
                is_active: true,
                is_focused: false,
                is_urgent: true,
            }],
        };
        let response = dispatch_with_workspaces(
            RpcRequest {
                jsonrpc: PROTOCOL_VERSION.into(),
                id: 4,
                method: "get_workspaces".into(),
            },
            &state,
        );
        let value = response.result.unwrap();
        assert_eq!(value["workspaces"][0]["id"], u64::MAX.to_string());
        assert_eq!(value["workspaces"][0]["is_focused"], false);
        assert_eq!(value["workspaces"][0]["is_urgent"], true);
    }
}

/// Resolve Core's socket location. Preview can override it with an absolute path
/// in a private directory, leaving the user's running Core untouched.
pub fn core_socket_path() -> Result<std::path::PathBuf, &'static str> {
    let configured = std::env::var_os("KIRA_CORE_SOCKET").map(std::path::PathBuf::from);
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").map(std::path::PathBuf::from);
    socket_path_from(configured.as_deref(), runtime.as_deref())
}

fn socket_path_from(
    configured: Option<&std::path::Path>,
    runtime: Option<&std::path::Path>,
) -> Result<std::path::PathBuf, &'static str> {
    if let Some(path) = configured {
        return if path.is_absolute() {
            Ok(path.to_path_buf())
        } else {
            Err("KIRA_CORE_SOCKET must be an absolute path")
        };
    }
    runtime
        .map(|dir| dir.join("kira-desktop.sock"))
        .ok_or("XDG_RUNTIME_DIR is required when KIRA_CORE_SOCKET is unset")
}

#[cfg(test)]
mod socket_path_tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn preview_overrides_default_socket() {
        let path = socket_path_from(
            Some(Path::new("/tmp/kira-preview-abc/core.sock")),
            Some(Path::new("/run/user/1000")),
        );
        assert_eq!(path.unwrap(), Path::new("/tmp/kira-preview-abc/core.sock"));
    }

    #[test]
    fn refuses_relative_preview_socket() {
        assert!(socket_path_from(Some(Path::new("core.sock")), None).is_err());
    }

    #[test]
    fn default_stays_inside_user_runtime_directory() {
        assert_eq!(
            socket_path_from(None, Some(Path::new("/run/user/1000"))).unwrap(),
            Path::new("/run/user/1000/kira-desktop.sock")
        );
    }
}

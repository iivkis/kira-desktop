//! Minimal newline-delimited JSON-RPC 2.0 contracts shared by Core and Shell.
//! Events and typed parameter schemas will be introduced in subsequent milestones.

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
    if request.jsonrpc != PROTOCOL_VERSION {
        return RpcResponse::failure(Some(request.id), -32600, "Invalid Request");
    }

    match request.method.as_str() {
        "ping" => RpcResponse::success(request.id, json!("pong")),
        "get_state" => RpcResponse::success(
            request.id,
            json!({ "version": env!("CARGO_PKG_VERSION"), "status": "bootstrap" }),
        ),
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

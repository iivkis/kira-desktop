//! Blocking, bounded Unix-socket client used by the Shell worker thread.
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

use serde::de::DeserializeOwned;

use crate::{RpcRequest, RpcResponse};

const MAX_RESPONSE_BYTES: u64 = 64 * 1024;

/// Sends a single JSON-RPC request and waits for a single newline-delimited reply.
/// Call this on a worker thread, never on the GTK main loop.
pub fn call(socket: &Path, request: &RpcRequest) -> io::Result<RpcResponse> {
    let mut stream = UnixStream::connect(socket)?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;

    serde_json::to_writer(&mut stream, request)?;
    stream.write_all(b"\n")?;
    stream.flush()?;

    let mut reader = BufReader::new(stream).take(MAX_RESPONSE_BYTES + 1);
    let mut reply = Vec::new();
    reader.read_until(b'\n', &mut reply)?;
    if reply.len() as u64 > MAX_RESPONSE_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "IPC response too large",
        ));
    }
    if !reply.ends_with(b"\n") {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "Incomplete IPC response",
        ));
    }
    let response: RpcResponse = serde_json::from_slice(&reply)?;
    if response.jsonrpc != crate::PROTOCOL_VERSION || response.id != Some(request.id) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Invalid JSON-RPC response id/version",
        ));
    }
    Ok(response)
}

/// Shared read-only JSON-RPC subscription transport. The public workspace
/// and config entrypoints keep their own types and method names.
fn subscribe<T: DeserializeOwned>(
    socket: &Path,
    method: &str,
    mut on_state: impl FnMut(T),
) -> io::Result<()> {
    let mut stream = UnixStream::connect(socket)?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let request = RpcRequest {
        params: None,
        jsonrpc: crate::PROTOCOL_VERSION.into(),
        id: 0,
        method: method.into(),
    };
    serde_json::to_writer(&mut stream, &request)?;
    stream.write_all(b"\n")?;
    stream.flush()?;

    let mut reader = BufReader::new(stream);
    loop {
        let mut line = Vec::new();
        reader
            .by_ref()
            .take(MAX_RESPONSE_BYTES + 1)
            .read_until(b'\n', &mut line)?;
        if line.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Core subscription closed",
            ));
        }
        if line.len() as u64 > MAX_RESPONSE_BYTES || !line.ends_with(b"\n") {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid subscription frame",
            ));
        }
        let reply: RpcResponse = serde_json::from_slice(&line)?;
        if reply.jsonrpc != crate::PROTOCOL_VERSION || reply.id != Some(0) || reply.error.is_some()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid subscription reply",
            ));
        }
        let state = reply.result.ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "Subscription result missing")
        })?;
        on_state(serde_json::from_value(state)?);
    }
}

/// Subscribe to live read-only workspace snapshots over one persistent
/// Core connection. The callback runs on the caller's thread, never GTK.
/// Returns on disconnect so the Shell can reconnect and mark the UI offline.
pub fn subscribe_workspaces<F: FnMut(crate::WorkspaceState)>(
    socket: &Path,
    on_state: F,
) -> io::Result<()> {
    subscribe::<crate::WorkspaceState>(socket, "subscribe_workspaces", on_state)
}

/// Internal read-only config stream. The Shell retains its last-good config
/// and reconnects after a disconnect. No JS or filesystem writes are exposed.
pub fn subscribe_config<F: FnMut(crate::ConfigState)>(
    socket: &Path,
    on_state: F,
) -> io::Result<()> {
    subscribe::<crate::ConfigState>(socket, "subscribe_config", on_state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PROTOCOL_VERSION;
    use std::os::unix::net::UnixListener;
    use std::thread;

    #[test]
    fn round_trip_over_real_unix_socket() {
        let dir = std::env::temp_dir().join(format!("kira-ipc-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("rpc.sock");
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path).unwrap();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream);
            let mut raw = String::new();
            reader.read_line(&mut raw).unwrap();
            let request: RpcRequest = serde_json::from_str(&raw).unwrap();
            let response = crate::dispatch(request);
            let mut stream = reader.into_inner();
            serde_json::to_writer(&mut stream, &response).unwrap();
            stream.write_all(b"\n").unwrap();
        });

        let response = call(
            &path,
            &RpcRequest {
                params: None,
                jsonrpc: PROTOCOL_VERSION.into(),
                id: 42,
                method: "ping".into(),
            },
        )
        .unwrap();
        assert_eq!(response.result, Some(serde_json::json!("pong")));
        server.join().unwrap();
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_dir(&dir).unwrap();
    }

    #[test]
    fn live_workspace_subscription_receives_multiple_snapshots() {
        let dir = std::env::temp_dir().join(format!("kira-sub-client-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ipc.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream);
            let mut raw = String::new();
            reader.read_line(&mut raw).unwrap();
            let request: RpcRequest = serde_json::from_str(&raw).unwrap();
            assert_eq!(request.method, "subscribe_workspaces");
            assert_eq!(request.id, 0);
            let mut stream = reader.into_inner();
            for state in [false, true] {
                let reply = crate::RpcResponse::success(
                    0,
                    serde_json::json!({
                        "connected": state, "workspaces": []
                    }),
                );
                serde_json::to_writer(&mut stream, &reply).unwrap();
                stream.write_all(b"\n").unwrap();
            }
        });
        let mut received = Vec::new();
        assert!(subscribe_workspaces(&path, |state| received.push(state.connected)).is_err());
        assert_eq!(received, vec![false, true]);
        server.join().unwrap();
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_dir(&dir).unwrap();
    }

    #[test]
    fn config_subscription_receives_validated_settings() {
        let dir = std::env::temp_dir().join(format!("kira-config-client-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("core.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let server = thread::spawn(move || {
            let (socket, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(socket);
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let request: RpcRequest = serde_json::from_str(&line).unwrap();
            assert_eq!(request.method, "subscribe_config");
            let mut socket = reader.into_inner();
            let state = crate::ConfigState {
                config: crate::UiConfig {
                    compact: true,
                    ..Default::default()
                },
                error: None,
            };
            let reply = crate::RpcResponse::success(0, serde_json::to_value(state).unwrap());
            serde_json::to_writer(&mut socket, &reply).unwrap();
            socket.write_all(b"\n").unwrap();
        });
        let mut seen = Vec::new();
        assert!(subscribe_config(&path, |state| seen.push(state.config.compact)).is_err());
        assert_eq!(seen, vec![true]);
        server.join().unwrap();
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }

    #[test]
    fn missing_socket_returns_error() {
        let response = call(
            Path::new("/nonexistent/kira-desktop.sock"),
            &RpcRequest {
                params: None,
                jsonrpc: PROTOCOL_VERSION.into(),
                id: 1,
                method: "ping".into(),
            },
        );
        assert!(response.is_err());
    }
}

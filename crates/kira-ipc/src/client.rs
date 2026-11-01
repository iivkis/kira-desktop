//! Blocking, bounded Unix-socket client used by the Shell worker thread.
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

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

/// Subscribe to live read-only workspace snapshots over one persistent
/// Core connection. Each JSON-RPC frame repeats the id of the subscription
/// request; the callback is invoked on the CALLER'S thread, not GTK's loop.
/// Returns on disconnect so the caller can reconnect and mark the UI offline.
pub fn subscribe_workspaces<F: FnMut(crate::WorkspaceState)>(
    socket: &Path,
    on_state: F,
) -> io::Result<()> {
    subscribe_workspaces_with_timeout(socket, on_state, crate::SUBSCRIPTION_READ_TIMEOUT)
}

fn subscribe_workspaces_with_timeout<F: FnMut(crate::WorkspaceState)>(
    socket: &Path,
    mut on_state: F,
    read_timeout: Duration,
) -> io::Result<()> {
    let mut stream = UnixStream::connect(socket)?;
    // Core heartbeats keep genuinely idle subscriptions alive. If Core hangs
    // while keeping its socket open, the timeout returns control to Shell,
    // whose existing reconnect loop clears the stale workspace snapshot.
    stream.set_read_timeout(Some(read_timeout))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let request = RpcRequest {
        params: None,
        jsonrpc: crate::PROTOCOL_VERSION.into(),
        id: 0,
        method: "subscribe_workspaces".into(),
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
                "Invalid workspace frame",
            ));
        }
        let reply: RpcResponse = serde_json::from_slice(&line)?;
        if reply.jsonrpc != crate::PROTOCOL_VERSION || reply.id != Some(0) || reply.error.is_some()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid workspace reply",
            ));
        }
        let state = reply.result.ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidData, "Workspace result missing")
        })?;
        if crate::is_subscription_heartbeat(&state) {
            continue; // Liveness is not a new workspace snapshot.
        }
        let state: crate::WorkspaceState = serde_json::from_value(state)?;
        on_state(state);
    }
}

/// Internal read-only config stream. Never called from the GTK main thread.
/// Returns on disconnect so the Shell can preserve last applied config and
/// reconnect. No JS command or filesystem write is exposed.
pub fn subscribe_config<F: FnMut(crate::ConfigState)>(
    socket: &Path,
    on_state: F,
) -> io::Result<()> {
    subscribe_config_with_timeout(socket, on_state, crate::SUBSCRIPTION_READ_TIMEOUT)
}

fn subscribe_config_with_timeout<F: FnMut(crate::ConfigState)>(
    socket: &Path,
    mut on_state: F,
    read_timeout: Duration,
) -> io::Result<()> {
    let mut stream = UnixStream::connect(socket)?;
    stream.set_read_timeout(Some(read_timeout))?;
    stream.set_write_timeout(Some(Duration::from_secs(2)))?;
    let request = RpcRequest {
        params: None,
        jsonrpc: crate::PROTOCOL_VERSION.into(),
        id: 0,
        method: "subscribe_config".into(),
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
                "Core config stream closed",
            ));
        }
        if line.len() as u64 > MAX_RESPONSE_BYTES || !line.ends_with(b"\n") {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid config frame",
            ));
        }
        let reply: RpcResponse = serde_json::from_slice(&line)?;
        if reply.jsonrpc != crate::PROTOCOL_VERSION || reply.id != Some(0) || reply.error.is_some()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid config response",
            ));
        }
        let value = reply
            .result
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Config result missing"))?;
        if crate::is_subscription_heartbeat(&value) {
            continue; // Keep the last known good config unchanged.
        }
        on_state(serde_json::from_value(value)?);
    }
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
    fn workspace_heartbeats_do_not_emit_duplicate_snapshots() {
        let directory = std::env::temp_dir().join(format!(
            "kira-ipc-heartbeat-ws-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("rpc.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream);
            let mut request = String::new();
            reader.read_line(&mut request).unwrap();
            assert!(request.contains("subscribe_workspaces"));
            let mut stream = reader.into_inner();
            for frame in [
                crate::RpcResponse::success(
                    0,
                    serde_json::json!({"connected": false, "workspaces": []}),
                ),
                crate::RpcResponse::heartbeat(0),
                crate::RpcResponse::heartbeat(0),
                crate::RpcResponse::success(
                    0,
                    serde_json::json!({"connected": true, "workspaces": []}),
                ),
            ] {
                serde_json::to_writer(&mut stream, &frame).unwrap();
                stream.write_all(b"\n").unwrap();
            }
        });
        let mut observed = Vec::new();
        assert!(subscribe_workspaces(&path, |state| observed.push(state.connected)).is_err());
        assert_eq!(observed, vec![false, true]);
        server.join().unwrap();
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn config_heartbeats_preserve_last_valid_settings() {
        let directory = std::env::temp_dir().join(format!(
            "kira-ipc-heartbeat-config-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("rpc.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream);
            let mut request = String::new();
            reader.read_line(&mut request).unwrap();
            assert!(request.contains("subscribe_config"));
            let mut stream = reader.into_inner();
            let state = crate::ConfigState::default();
            for frame in [
                crate::RpcResponse::success(0, serde_json::to_value(&state).unwrap()),
                crate::RpcResponse::heartbeat(0),
                crate::RpcResponse::heartbeat(0),
            ] {
                serde_json::to_writer(&mut stream, &frame).unwrap();
                stream.write_all(b"\n").unwrap();
            }
        });
        let mut observed = Vec::new();
        assert!(subscribe_config(&path, |state| observed.push(state)).is_err());
        assert_eq!(observed, vec![crate::ConfigState::default()]);
        server.join().unwrap();
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn silent_but_open_workspace_socket_times_out() {
        let directory = std::env::temp_dir().join(format!(
            "kira-ipc-silent-ws-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("rpc.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream);
            let mut request = String::new();
            reader.read_line(&mut request).unwrap();
            let mut stream = reader.into_inner();
            let frame = crate::RpcResponse::success(
                0,
                serde_json::json!({"connected": true, "workspaces": []}),
            );
            serde_json::to_writer(&mut stream, &frame).unwrap();
            stream.write_all(b"\n").unwrap();
            thread::sleep(Duration::from_millis(500)); // TCP/Unix connection stays open.
        });
        let mut observed = Vec::new();
        let error = subscribe_workspaces_with_timeout(
            &path,
            |state| observed.push(state.connected),
            Duration::from_millis(150),
        ).unwrap_err();
        assert!(matches!(error.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut));
        assert_eq!(observed, vec![true]);
        server.join().unwrap();
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn silent_but_open_config_socket_times_out() {
        let directory = std::env::temp_dir().join(format!(
            "kira-ipc-silent-config-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("rpc.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream);
            let mut request = String::new();
            reader.read_line(&mut request).unwrap();
            let mut stream = reader.into_inner();
            let frame = crate::RpcResponse::success(
                0,
                serde_json::to_value(crate::ConfigState::default()).unwrap(),
            );
            serde_json::to_writer(&mut stream, &frame).unwrap();
            stream.write_all(b"\n").unwrap();
            thread::sleep(Duration::from_millis(500));
        });
        let mut observed = Vec::new();
        let error = subscribe_config_with_timeout(
            &path,
            |state| observed.push(state),
            Duration::from_millis(150),
        ).unwrap_err();
        assert!(matches!(error.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut));
        assert_eq!(observed, vec![crate::ConfigState::default()]);
        server.join().unwrap();
        std::fs::remove_dir_all(directory).unwrap();
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

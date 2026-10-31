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
        return Err(io::Error::new(io::ErrorKind::InvalidData, "IPC response too large"));
    }
    if !reply.ends_with(b"\n") {
        return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "Incomplete IPC response"));
    }
    let response: RpcResponse = serde_json::from_slice(&reply)?;
    if response.jsonrpc != crate::PROTOCOL_VERSION || response.id != Some(request.id) {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid JSON-RPC response id/version"));
    }
    Ok(response)
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

        let response = call(&path, &RpcRequest {
            jsonrpc: PROTOCOL_VERSION.into(), id: 42, method: "ping".into()
        }).unwrap();
        assert_eq!(response.result, Some(serde_json::json!("pong")));
        server.join().unwrap();
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_dir(&dir).unwrap();
    }

    #[test]
    fn missing_socket_returns_error() {
        let response = call(Path::new("/nonexistent/kira-desktop.sock"), &RpcRequest {
            jsonrpc: PROTOCOL_VERSION.into(), id: 1, method: "ping".into()
        });
        assert!(response.is_err());
    }
}

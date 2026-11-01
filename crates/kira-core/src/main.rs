//! Core state daemon: read-only subscriptions plus one checked Niri focus action.

mod config;
mod niri;

use std::error::Error;
use std::io;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::{Path, PathBuf};

use kira_ipc::{ConfigState, RpcRequest, RpcResponse, WorkspaceState, dispatch_with_state};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::watch;

const MAX_REQUEST_BYTES: usize = 4096;

/// Delete only the socket inode owned by this Core instance.
struct SocketGuard {
    path: PathBuf,
    device: u64,
    inode: u64,
}

impl SocketGuard {
    fn new(path: &Path) -> io::Result<Self> {
        let metadata = std::fs::symlink_metadata(path)?;
        if !metadata.file_type().is_socket() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Not a Unix socket",
            ));
        }
        Ok(Self {
            path: path.to_owned(),
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }
}

impl Drop for SocketGuard {
    fn drop(&mut self) {
        let Ok(metadata) = std::fs::symlink_metadata(&self.path) else {
            return;
        };
        if metadata.file_type().is_socket()
            && metadata.dev() == self.device
            && metadata.ino() == self.inode
            && let Err(error) = std::fs::remove_file(&self.path)
        {
            eprintln!("kira-core: cannot remove owned socket: {error}");
        }
    }
}

/// Read newline-delimited IPC without growing beyond the configured bound.
async fn read_bounded_line<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    max_bytes: usize,
) -> io::Result<Option<Vec<u8>>> {
    let mut line = Vec::new();
    loop {
        let buffer = reader.fill_buf().await?;
        if buffer.is_empty() {
            return if line.is_empty() {
                Ok(None)
            } else {
                Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "Incomplete IPC request",
                ))
            };
        }
        let count = buffer
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(buffer.len(), |position| position + 1);
        if count > max_bytes.saturating_sub(line.len()) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "IPC request too large",
            ));
        }
        line.extend_from_slice(&buffer[..count]);
        let complete = buffer[count - 1] == b'\n';
        reader.consume(count);
        if complete {
            return Ok(Some(line));
        }
    }
}

/// Stream the latest read-only state over a single subscribed JSON-RPC
/// connection. Workspace and config channels share identical framing.
async fn stream_state<T: Clone + serde::Serialize>(
    writer: &mut tokio::net::unix::OwnedWriteHalf,
    mut updates: watch::Receiver<T>,
    id: u64,
) -> io::Result<()> {
    loop {
        let snapshot = updates.borrow_and_update().clone();
        let response = RpcResponse::success(id, serde_json::to_value(snapshot)?);
        writer.write_all(&serde_json::to_vec(&response)?).await?;
        writer.write_all(b"\\n").await?;
        if updates.changed().await.is_err() {
            return Ok(());
        }
    }
}

async fn serve(
    stream: UnixStream,
    workspaces: watch::Receiver<WorkspaceState>,
    config: watch::Receiver<ConfigState>,
    niri_socket: Option<&Path>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let (read_half, mut writer) = stream.into_split();
    let mut reader = BufReader::new(read_half);

    while let Some(line) = read_bounded_line(&mut reader, MAX_REQUEST_BYTES).await? {
        let response = match serde_json::from_slice::<RpcRequest>(&line) {
            Ok(request)
                if request.method == "subscribe_workspaces"
                    && request.jsonrpc == kira_ipc::PROTOCOL_VERSION =>
            {
                // Internal streaming endpoint, not exposed through the WebKit
                // JS RPC allowlist; one subscription per socket connection.
                return Ok(stream_state(&mut writer, workspaces, request.id).await?);
            }
            Ok(request)
                if request.method == "subscribe_config"
                    && request.jsonrpc == kira_ipc::PROTOCOL_VERSION =>
            {
                return Ok(stream_state(&mut writer, config, request.id).await?);
            }
            Ok(request)
                if request.method == "focus_workspace"
                    && request.jsonrpc == kira_ipc::PROTOCOL_VERSION =>
            {
                // This is intentionally the sole Niri mutation available
                // through Core. The ID must exist in a connected snapshot.
                let state = workspaces.borrow().clone();
                match niri::validated_focus_id(request.params.as_ref(), &state) {
                    Err(message) => RpcResponse::failure(Some(request.id), -32602, message),
                    Ok(id) => match niri_socket {
                        Some(path) => match niri::focus_workspace(path, id).await {
                            Ok(()) => RpcResponse::success(
                                request.id,
                                serde_json::json!({"accepted": true}),
                            ),
                            Err(error) => {
                                eprintln!("kira-core: Niri workspace focus failed: {error}");
                                RpcResponse::failure(
                                    Some(request.id),
                                    -32000,
                                    "Niri workspace focus failed",
                                )
                            }
                        },
                        None => RpcResponse::failure(Some(request.id), -32000, "Niri unavailable"),
                    },
                }
            }
            Ok(request) => dispatch_with_state(
                request,
                &workspaces.borrow().clone(),
                &config.borrow().clone(),
            ),
            Err(_) => RpcResponse::failure(None, -32700, "Parse error"),
        };
        let encoded = serde_json::to_vec(&response)?;
        writer.write_all(&encoded).await?;
        writer.write_all(b"\n").await?;
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Pure validation command: no GUI, Core socket or compositor required.
    let mut args = std::env::args_os().skip(1);
    if let Some(option) = args.next() {
        if option != "--check-config" {
            return Err("Usage: kira-core [--check-config /absolute/path/config.kdl]".into());
        }
        let path = args.next().ok_or("--check-config requires a file path")?;
        if args.next().is_some() {
            return Err("--check-config accepts exactly one file path".into());
        }
        let path = PathBuf::from(path);
        if !path.is_absolute() {
            return Err("--check-config requires an absolute path".into());
        }
        let settings = config::load(&path).map_err(io::Error::other)?;
        println!("{}", serde_json::to_string_pretty(&settings)?);
        return Ok(());
    }

    let path = kira_ipc::core_socket_path()?;
    // Binding never unlinks an existing socket: another Core may own it.
    let listener = UnixListener::bind(&path)?;
    let _guard = SocketGuard::new(&path)?;
    eprintln!("kira-core listening on {}", path.display());

    let (config_sender, config_receiver) = watch::channel(ConfigState::default());
    match config::config_path() {
        Ok(path) => {
            eprintln!("kira-core: watching KDL config at {}", path.display());
            tokio::spawn(config::monitor(path, config_sender));
        }
        Err(error) => {
            config_sender.send_replace(ConfigState {
                config: Default::default(),
                error: Some(error),
            });
        }
    }

    let (workspace_sender, workspace_receiver) = watch::channel(WorkspaceState::default());
    let niri_socket = niri::socket_path_from(std::env::var_os("NIRI_SOCKET"));
    if let Some(socket) = niri_socket.clone() {
        tokio::spawn(niri::monitor(socket, workspace_sender.clone()));
    } else {
        eprintln!("kira-core: no valid NIRI_SOCKET; workspace state will be offline");
    }

    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let (stream, _) = accepted?;
                let state = workspace_receiver.clone();
                let config = config_receiver.clone();
                let socket = niri_socket.clone();
                tokio::spawn(async move {
                    if let Err(error) = serve(stream, state, config, socket.as_deref()).await {
                        eprintln!("kira-core client error: {error}");
                    }
                });
            }
            _ = tokio::signal::ctrl_c() => break,
            _ = terminate.recv() => break,
        }
    }
    eprintln!("kira-core shutting down");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn bounded_reader_preserves_consecutive_messages() {
        let input = b"one\nnext\n".as_slice();
        let mut reader = BufReader::new(input);
        assert_eq!(
            read_bounded_line(&mut reader, 8).await.unwrap(),
            Some(b"one\n".to_vec())
        );
        assert_eq!(
            read_bounded_line(&mut reader, 8).await.unwrap(),
            Some(b"next\n".to_vec())
        );
        assert_eq!(read_bounded_line(&mut reader, 8).await.unwrap(), None);
    }

    #[tokio::test]
    async fn bounded_reader_rejects_long_requests() {
        let input = b"123456789\n".as_slice();
        let mut reader = BufReader::new(input);
        let error = read_bounded_line(&mut reader, 8).await.unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }

    #[tokio::test]
    async fn bounded_reader_rejects_unterminated_requests() {
        let input = b"not-terminated".as_slice();
        let mut reader = BufReader::new(input);
        let error = read_bounded_line(&mut reader, 32).await.unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
    }

    #[tokio::test]
    async fn subscription_streams_initial_snapshot_and_focus_changes() {
        use tokio::io::{AsyncBufReadExt, BufReader as TokioBufReader};
        let dir = std::env::temp_dir().join(format!("kira-subscribe-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ipc.sock");
        let listener = tokio::net::UnixListener::bind(&path).unwrap();
        let (tx, rx) = watch::channel(WorkspaceState::default());
        let task = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (_tx_config, rx_config) = watch::channel(ConfigState::default());
            let _ = serve(stream, rx, rx_config, None).await;
        });
        let mut conn = UnixStream::connect(&path).await.unwrap();
        conn.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":77,\"method\":\"subscribe_workspaces\"}\n")
            .await
            .unwrap();
        let mut reader = TokioBufReader::new(conn);
        let mut line = String::new();
        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            reader.read_line(&mut line),
        )
        .await
        .unwrap()
        .unwrap();
        let first: RpcResponse = serde_json::from_str(&line).unwrap();
        assert_eq!(first.id, Some(77));
        assert_eq!(first.result.unwrap()["connected"], false);
        let next = WorkspaceState {
            connected: true,
            workspaces: vec![kira_ipc::Workspace {
                id: "123".into(),
                idx: 2,
                name: None,
                output: Some("winit".into()),
                is_active: true,
                is_focused: true,
                is_urgent: false,
            }],
        };
        tx.send_replace(next);
        line.clear();
        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            reader.read_line(&mut line),
        )
        .await
        .unwrap()
        .unwrap();
        let update: RpcResponse = serde_json::from_str(&line).unwrap();
        assert_eq!(update.id, Some(77));
        assert_eq!(update.result.unwrap()["workspaces"][0]["idx"], 2);
        drop(reader);
        task.abort();
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_dir(&dir).unwrap();
    }

    #[tokio::test]
    async fn config_subscription_delivers_latest_valid_snapshot() {
        use tokio::io::AsyncBufReadExt;
        let dir = std::env::temp_dir().join(format!("kira-config-sub-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("core.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let (_ws_tx, ws_rx) = watch::channel(WorkspaceState::default());
        let (cfg_tx, cfg_rx) = watch::channel(ConfigState::default());
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let _ = serve(stream, ws_rx, cfg_rx, None).await;
        });
        let mut stream = UnixStream::connect(&path).await.unwrap();
        stream
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":88,\"method\":\"subscribe_config\"}\n")
            .await
            .unwrap();
        let mut reader = BufReader::new(stream);
        let mut first = String::new();
        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            reader.read_line(&mut first),
        )
        .await
        .unwrap()
        .unwrap();
        let first: RpcResponse = serde_json::from_str(&first).unwrap();
        assert_eq!(first.result.unwrap()["config"]["compact"], false);
        cfg_tx.send_replace(ConfigState {
            config: kira_ipc::UiConfig {
                compact: true,
                ..Default::default()
            },
            error: None,
        });
        let mut second = String::new();
        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            reader.read_line(&mut second),
        )
        .await
        .unwrap()
        .unwrap();
        let second: RpcResponse = serde_json::from_str(&second).unwrap();
        assert_eq!(second.result.unwrap()["config"]["compact"], true);
        server.abort();
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_dir(&dir).unwrap();
    }

    #[tokio::test]
    async fn focus_rpc_refuses_unknown_workspace_before_niri_action() {
        let dir = std::env::temp_dir().join(format!("kira-focus-rpc-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("core.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let (_ws_tx, ws_rx) = watch::channel(WorkspaceState::default());
        let (_cfg_tx, cfg_rx) = watch::channel(ConfigState::default());
        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let _ = serve(stream, ws_rx, cfg_rx, None).await;
        });
        let mut stream = UnixStream::connect(&path).await.unwrap();
        stream.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":77,\"method\":\"focus_workspace\",\"params\":{\"id\":\"9\"}}\n").await.unwrap();
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            tokio::io::AsyncBufReadExt::read_line(&mut reader, &mut line),
        )
        .await
        .unwrap()
        .unwrap();
        let response: RpcResponse = serde_json::from_str(&line).unwrap();
        assert_eq!(response.error.unwrap().code, -32602);
        server.abort();
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_dir(&dir).unwrap();
    }

    #[test]
    fn guard_removes_owned_socket() {
        let directory = std::env::temp_dir().join(format!(
            "kira-core-guard-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("socket");
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let guard = SocketGuard::new(&path).unwrap();
        drop(listener);
        drop(guard);
        assert!(!path.exists());
        std::fs::remove_dir(&directory).unwrap();
    }

    #[test]
    fn guard_preserves_replacement_socket() {
        let directory = std::env::temp_dir().join(format!(
            "kira-core-replacement-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("socket");
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let guard = SocketGuard::new(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        let replacement = std::os::unix::net::UnixListener::bind(&path).unwrap();
        drop(listener);
        drop(guard);
        assert!(path.exists());
        drop(replacement);
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_dir(&directory).unwrap();
    }
}

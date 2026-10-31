//! Standalone read-only system-state daemon with Niri workspace IPC.

mod niri;

use std::error::Error;
use std::io;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::{Path, PathBuf};

use kira_ipc::{RpcRequest, RpcResponse, WorkspaceState, dispatch_with_workspaces};
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

async fn serve(
    stream: UnixStream,
    workspaces: watch::Receiver<WorkspaceState>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let (read_half, mut writer) = stream.into_split();
    let mut reader = BufReader::new(read_half);

    while let Some(line) = read_bounded_line(&mut reader, MAX_REQUEST_BYTES).await? {
        let response = match serde_json::from_slice::<RpcRequest>(&line) {
            Ok(request) => dispatch_with_workspaces(request, &workspaces.borrow().clone()),
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
    let path = kira_ipc::core_socket_path()?;
    // Binding never unlinks an existing socket: another Core may own it.
    let listener = UnixListener::bind(&path)?;
    let _guard = SocketGuard::new(&path)?;
    eprintln!("kira-core listening on {}", path.display());

    let (workspace_sender, workspace_receiver) = watch::channel(WorkspaceState::default());
    if let Some(socket) = niri::socket_path_from(std::env::var_os("NIRI_SOCKET")) {
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
                tokio::spawn(async move {
                    if let Err(error) = serve(stream, state).await {
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

//! Standalone system-state daemon. This milestone exposes ping/get_state only.
use std::error::Error;

use kira_ipc::{RpcRequest, RpcResponse, dispatch};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};

async fn serve(stream: UnixStream) -> Result<(), Box<dyn Error + Send + Sync>> {
    let (read_half, mut writer) = stream.into_split();
    let mut lines = BufReader::new(read_half).lines();

    while let Some(line) = lines.next_line().await? {
        // One JSON-RPC request per line; a malformed request cannot crash Core.
        let response = match serde_json::from_str::<RpcRequest>(&line) {
            Ok(request) => dispatch(request),
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
    // Do not unlink an existing socket: another Core instance may own it.
    let listener = UnixListener::bind(&path)?;
    eprintln!("kira-core listening on {}", path.display());

    loop {
        let (stream, _) = listener.accept().await?;
        tokio::spawn(async move {
            if let Err(error) = serve(stream).await {
                eprintln!("kira-core client error: {error}");
            }
        });
    }
}

//! Read-only Niri event stream. No Action or output-changing IPC is sent.
//! A full workspace snapshot is received first; incremental events then
//! update Core's local state. Disconnect clears state; reconnect resyncs.

use std::io;
use std::os::unix::fs::FileTypeExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use kira_ipc::{Workspace, WorkspaceState};
use serde_json::Value;
use tokio::io::{AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::watch;

use crate::read_bounded_line;

const MAX_NIRI_LINE: usize = 256 * 1024;
const MAX_WORKSPACES: usize = 256;
const RETRY_DELAY: Duration = Duration::from_secs(1);

pub fn socket_path_from(value: Option<std::ffi::OsString>) -> Option<PathBuf> {
    value.map(PathBuf::from).filter(|path| path.is_absolute())
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn parse_workspaces(value: &Value) -> io::Result<Vec<Workspace>> {
    let list = value
        .as_array()
        .ok_or_else(|| invalid("workspaces not an array"))?;
    if list.len() > MAX_WORKSPACES {
        return Err(invalid("too many workspaces"));
    }
    let mut result = Vec::with_capacity(list.len());
    for item in list {
        let id = item["id"]
            .as_u64()
            .ok_or_else(|| invalid("workspace id missing"))?;
        let idx = u8::try_from(
            item["idx"]
                .as_u64()
                .ok_or_else(|| invalid("workspace idx missing"))?,
        )
        .map_err(|_| invalid("invalid workspace idx"))?;
        let name = match item.get("name") {
            None | Some(Value::Null) => None,
            Some(Value::String(text)) => Some(text.clone()),
            _ => return Err(invalid("invalid workspace name")),
        };
        let output = match item.get("output") {
            None | Some(Value::Null) => None,
            Some(Value::String(text)) => Some(text.clone()),
            _ => return Err(invalid("invalid workspace output")),
        };
        result.push(Workspace {
            id: id.to_string(),
            idx,
            name,
            output,
            is_active: item["is_active"]
                .as_bool()
                .ok_or_else(|| invalid("is_active missing"))?,
            is_focused: item["is_focused"]
                .as_bool()
                .ok_or_else(|| invalid("is_focused missing"))?,
            is_urgent: item["is_urgent"]
                .as_bool()
                .ok_or_else(|| invalid("is_urgent missing"))?,
        });
    }
    result.sort_by(|a, b| {
        a.output
            .cmp(&b.output)
            .then(a.idx.cmp(&b.idx))
            .then(a.id.cmp(&b.id))
    });
    Ok(result)
}

/// Apply only the workspace-related variants. Unknown future Niri events are ignored.
/// A malformed supported event is a protocol error and forces a resync.
fn apply_event(state: &mut WorkspaceState, value: &Value) -> io::Result<bool> {
    if let Some(payload) = value.get("WorkspacesChanged") {
        let workspaces = parse_workspaces(&payload["workspaces"])?;
        *state = WorkspaceState {
            connected: true,
            workspaces,
        };
        return Ok(true);
    }
    if !state.connected {
        return Ok(false);
    }
    if let Some(payload) = value.get("WorkspaceActivated") {
        let id = payload["id"]
            .as_u64()
            .ok_or_else(|| invalid("activation id missing"))?
            .to_string();
        let focused = payload["focused"]
            .as_bool()
            .ok_or_else(|| invalid("activation focused missing"))?;
        let Some(target) = state.workspaces.iter().find(|ws| ws.id == id) else {
            // Unknown workspace: wait for the next complete WorkspacesChanged.
            return Ok(false);
        };
        let output = target.output.clone();
        for workspace in &mut state.workspaces {
            if workspace.id == id {
                workspace.is_active = true;
                if focused {
                    workspace.is_focused = true;
                }
            } else {
                if workspace.output == output {
                    workspace.is_active = false;
                }
                if focused {
                    workspace.is_focused = false;
                }
            }
        }
        return Ok(true);
    }
    if let Some(payload) = value.get("WorkspaceUrgencyChanged") {
        let id = payload["id"]
            .as_u64()
            .ok_or_else(|| invalid("urgency id missing"))?
            .to_string();
        let urgent = payload["urgent"]
            .as_bool()
            .ok_or_else(|| invalid("urgency flag missing"))?;
        if let Some(workspace) = state.workspaces.iter_mut().find(|ws| ws.id == id) {
            workspace.is_urgent = urgent;
            return Ok(true);
        }
    }
    Ok(false)
}

async fn run_stream(socket: &Path, sender: &watch::Sender<WorkspaceState>) -> io::Result<()> {
    let stream = tokio::time::timeout(Duration::from_secs(3), UnixStream::connect(socket))
        .await
        .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "niri connect timeout"))??;
    let (reader, mut writer) = stream.into_split();
    writer.write_all(b"\"EventStream\"\n").await?;
    let mut reader = BufReader::new(reader);

    let ack = tokio::time::timeout(
        Duration::from_secs(3),
        read_bounded_line(&mut reader, MAX_NIRI_LINE),
    )
    .await
    .map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "niri handshake timeout"))??
    .ok_or_else(|| invalid("niri closed before handshake"))?;
    let ack: Value = serde_json::from_slice(&ack)?;
    if ack != serde_json::json!({"Ok": "Handled"}) {
        return Err(invalid("niri rejected EventStream"));
    }
    let mut state = WorkspaceState::default();
    loop {
        let line = read_bounded_line(&mut reader, MAX_NIRI_LINE)
            .await?
            .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "niri disconnected"))?;
        let event: Value = serde_json::from_slice(&line)?;
        if apply_event(&mut state, &event)? {
            sender.send_replace(state.clone());
        }
    }
}

/// Monitor the exact supplied NIRI_SOCKET, reconnecting on disconnect.
/// Invalid paths are never substituted with a host/default compositor socket.
pub async fn monitor(socket: PathBuf, sender: watch::Sender<WorkspaceState>) {
    loop {
        if std::fs::metadata(&socket).is_ok_and(|metadata| metadata.file_type().is_socket())
            && let Err(error) = run_stream(&socket, &sender).await
        {
            eprintln!("kira-core: Niri IPC unavailable: {error}");
        }
        // Never keep stale workspace state after an IPC failure.
        sender.send_replace(WorkspaceState::default());
        tokio::time::sleep(RETRY_DELAY).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
    use tokio::net::UnixListener;

    fn ws(id: u64, idx: u8, output: &str, focused: bool) -> Value {
        json!({
            "id": id, "idx": idx, "name": null, "output": output,
            "is_active": focused, "is_focused": focused, "is_urgent": false,
            "active_window_id": null, "future_extra_field": "ignored"
        })
    }

    #[test]
    fn ignores_unknown_fields_and_preserves_multimonitor_id_index() {
        let result =
            parse_workspaces(&json!([ws(90, 2, "HDMI", false), ws(4, 1, "DP", true)])).unwrap();
        assert_eq!(result[0].id, "4");
        assert_eq!(result[1].idx, 2);
        assert_eq!(result[0].output.as_deref(), Some("DP"));
    }

    #[test]
    fn focus_activation_and_urgency_update_without_resetting_other_output() {
        let mut state = WorkspaceState::default();
        apply_event(
            &mut state,
            &json!({"WorkspacesChanged": {
                "workspaces": [ws(1, 1, "DP", true), ws(2, 2, "DP", false), ws(3, 1, "HDMI", false)]
            }}),
        )
        .unwrap();
        assert!(state.connected);
        assert!(
            apply_event(
                &mut state,
                &json!({"WorkspaceActivated": {"id": 2, "focused": true}})
            )
            .unwrap()
        );
        assert!(!state.workspaces[0].is_active);
        assert!(state.workspaces[1].is_active);
        assert!(state.workspaces[1].is_focused);
        assert!(!state.workspaces[2].is_focused);
        assert!(
            apply_event(
                &mut state,
                &json!({"WorkspaceUrgencyChanged": {"id": 3, "urgent": true}})
            )
            .unwrap()
        );
        assert!(state.workspaces[2].is_urgent);
        assert!(
            apply_event(
                &mut state,
                &json!({"WorkspaceActivated": {"id": 3, "focused": false}})
            )
            .unwrap()
        );
        assert!(state.workspaces[1].is_focused); // activation without focus does not change global focus
    }

    #[test]
    fn rejects_malformed_workspace_payload_without_fake_connected_state() {
        let mut state = WorkspaceState::default();
        assert!(
            apply_event(
                &mut state,
                &json!({"WorkspacesChanged": {"workspaces": [{"id": 1}]}})
            )
            .is_err()
        );
        assert!(!state.connected);
        assert!(socket_path_from(Some("relative.sock".into())).is_none());
        assert!(socket_path_from(None).is_none());
    }

    #[tokio::test]
    async fn mocked_socket_sends_only_event_stream_and_applies_updates() {
        let dir = std::env::temp_dir().join(format!(
            "kira-niri-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("niri.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let task = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let (reader, mut writer) = stream.into_split();
            let mut reader = BufReader::new(reader);
            let mut request = String::new();
            reader.read_line(&mut request).await.unwrap();
            assert_eq!(request, "\"EventStream\"\n");
            writer.write_all(b"{\"Ok\":\"Handled\"}\n").await.unwrap();
            writer
                .write_all(
                    (json!({"WorkspacesChanged": {"workspaces": [ws(70, 1, "winit", true)]}})
                        .to_string()
                        + "\n")
                        .as_bytes(),
                )
                .await
                .unwrap();
            tokio::time::sleep(Duration::from_millis(40)).await;
            writer
                .write_all(b"{\"WorkspaceUrgencyChanged\":{\"id\":70,\"urgent\":true}}\n")
                .await
                .unwrap();
        });
        let (tx, mut rx) = watch::channel(WorkspaceState::default());
        let result = tokio::time::timeout(Duration::from_secs(2), run_stream(&path, &tx))
            .await
            .unwrap();
        assert!(result.is_err()); // mock closed its IPC socket
        rx.changed().await.unwrap();
        let last = rx.borrow().clone();
        assert!(last.connected);
        assert_eq!(last.workspaces[0].output.as_deref(), Some("winit"));
        assert!(last.workspaces[0].is_urgent);
        task.await.unwrap();
        std::fs::remove_file(&path).unwrap();
        std::fs::remove_dir(&dir).unwrap();
    }
}

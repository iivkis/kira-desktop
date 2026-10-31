//! Niri EventStream plus one deliberately narrow click-to-focus action.
//! Full snapshots and changes are read-only. Only a known workspace ID can
//! be focused; arbitrary actions, windows, outputs and configuration writes
//! are never accepted from the UI. Disconnect clears state and reconnects.

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

/// Only accept a stable workspace ID already advertised by the connected
/// compositor. Indexes are per-output and must never be used for clicks.
pub fn validated_focus_id(params: Option<&Value>, state: &WorkspaceState) -> Result<u64, &'static str> {
    let params = params.and_then(Value::as_object).ok_or("Expected focus params")?;
    if params.len() != 1 {
        return Err("Only workspace id is allowed");
    }
    let id = params.get("id").and_then(Value::as_str).ok_or("Workspace id must be a string")?;
    let parsed = id.parse::<u64>().map_err(|_| "Invalid workspace id")?;
    if parsed.to_string() != id {
        return Err("Non-canonical workspace id");
    }
    if !state.connected {
        return Err("Niri not connected");
    }
    if !state.workspaces.iter().any(|workspace| workspace.id == id) {
        return Err("Unknown workspace id");
    }
    Ok(parsed)
}

/// Sends the *only* supported mutating Niri request, FocusWorkspace by
/// stable u64 ID. Never use a workspace idx or a guessed/default socket.
pub async fn focus_workspace(socket: &Path, workspace_id: u64) -> io::Result<()> {
    let metadata = std::fs::symlink_metadata(socket)?;
    if !metadata.file_type().is_socket() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "Niri socket path is not a socket"));
    }
    let action = serde_json::json!({
        "Action": {"FocusWorkspace": {"reference": {"Id": workspace_id}}}
    });
    tokio::time::timeout(Duration::from_secs(3), async {
        let mut stream = UnixStream::connect(socket).await?;
        stream.write_all(action.to_string().as_bytes()).await?;
        stream.write_all(b"\n").await?;
        let mut reader = BufReader::new(stream);
        let line = read_bounded_line(&mut reader, 4096).await?
            .ok_or_else(|| invalid("Niri closed without action response"))?;
        let reply: Value = serde_json::from_slice(&line)?;
        if reply == serde_json::json!({"Ok": "Handled"}) {
            Ok(())
        } else {
            Err(invalid("Niri rejected workspace focus"))
        }
    }).await.map_err(|_| io::Error::new(io::ErrorKind::TimedOut, "Niri action timed out"))?
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

    #[test]
    fn focus_rejects_offline_unknown_noncanonical_and_non_string_ids() {
        let state = WorkspaceState { connected: true, workspaces: vec![Workspace {
            id: u64::MAX.to_string(), idx: 2, name: None, output: Some("DP".into()),
            is_active: false, is_focused: false, is_urgent: false,
        }] };
        assert_eq!(validated_focus_id(Some(&json!({"id": u64::MAX.to_string()})), &state), Ok(u64::MAX));
        for invalid in [json!({"id":2}), json!({"id":"02"}), json!({"idx":2}),
            json!({"id":"2"}), json!({"id":"-1"}), json!({"id":"18446744073709551616"}),
            json!({"id": u64::MAX.to_string(), "action":"quit"})]
        {
            assert!(validated_focus_id(Some(&invalid), &state).is_err(), "{invalid}");
        }
        assert!(validated_focus_id(None, &state).is_err());
        assert!(validated_focus_id(Some(&json!({"id":u64::MAX.to_string()})), &WorkspaceState::default()).is_err());
    }

    #[tokio::test]
    async fn focus_action_targets_exact_workspace_id_and_handles_niri_error() {
        let dir = std::env::temp_dir().join(format!("kira-focus-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("niri.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let server = tokio::spawn(async move {
            for ok in [true, false] {
                let (socket, _) = listener.accept().await.unwrap();
                let (reader, mut writer) = socket.into_split();
                let mut reader = BufReader::new(reader);
                let line = read_bounded_line(&mut reader, 4096).await.unwrap().unwrap();
                let request: Value = serde_json::from_slice(&line).unwrap();
                assert_eq!(request, json!({"Action":{"FocusWorkspace":{"reference":{"Id":u64::MAX}}}}));
                writer.write_all(if ok { b"{\"Ok\":\"Handled\"}\n" } else { b"{\"Err\":\"No such workspace\"}\n" }).await.unwrap();
            }
        });
        assert!(focus_workspace(&path, u64::MAX).await.is_ok());
        assert!(focus_workspace(&path, u64::MAX).await.is_err());
        server.await.unwrap();
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(dir).unwrap();
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

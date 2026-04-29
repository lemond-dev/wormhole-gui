//! Tauri IPC commands. Architecture §6.1.

use crate::bridge::{start_event_pump, SessionState};
use serde::Deserialize;
use std::path::PathBuf;
use std::str::FromStr;
use tauri::{AppHandle, State};
use wormhole_gui_core::{spawn_session_thread, Cmd, Role};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionMode {
    Send,
    Recv,
}

fn cmd_tx(state: &SessionState) -> Result<async_channel::Sender<Cmd>, String> {
    let guard = state.handle.lock().unwrap();
    Ok(guard
        .as_ref()
        .ok_or_else(|| "no active session".to_string())?
        .cmd_tx
        .clone())
}

#[tauri::command]
pub async fn start_session(
    app: AppHandle,
    state: State<'_, SessionState>,
    mode: SessionMode,
    code: Option<String>,
) -> Result<(), String> {
    {
        let guard = state.handle.lock().unwrap();
        if guard.is_some() {
            return Err("session already running".into());
        }
    }
    let role = match mode {
        SessionMode::Send => Role::Allocator,
        SessionMode::Recv => Role::Joiner,
    };
    let handle = spawn_session_thread(role);
    let evt_rx = handle.evt_rx.clone();

    if let SessionMode::Recv = mode {
        let code_str = code.ok_or_else(|| "code required for recv mode".to_string())?;
        let parsed = magic_wormhole::Code::from_str(&code_str)
            .map_err(|e| format!("invalid code: {e}"))?;
        handle
            .cmd_tx
            .send(Cmd::JoinCode(parsed))
            .await
            .map_err(|_| "session thread closed".to_string())?;
    }

    start_event_pump(app, evt_rx);
    *state.handle.lock().unwrap() = Some(handle);
    Ok(())
}

#[tauri::command]
pub async fn send_text(state: State<'_, SessionState>, content: String) -> Result<(), String> {
    cmd_tx(&state)?
        .send(Cmd::SendText(content))
        .await
        .map_err(|_| "session thread closed".into())
}

#[tauri::command]
pub async fn send_file(state: State<'_, SessionState>, path: String) -> Result<(), String> {
    cmd_tx(&state)?
        .send(Cmd::SendFile { path: PathBuf::from(path) })
        .await
        .map_err(|_| "session thread closed".into())
}

#[tauri::command]
pub async fn accept_file(state: State<'_, SessionState>, id: String) -> Result<(), String> {
    cmd_tx(&state)?
        .send(Cmd::AcceptFile { id })
        .await
        .map_err(|_| "session thread closed".into())
}

#[tauri::command]
pub async fn reject_file(
    state: State<'_, SessionState>,
    id: String,
    reason: Option<String>,
) -> Result<(), String> {
    cmd_tx(&state)?
        .send(Cmd::RejectFile {
            id,
            reason: reason.unwrap_or_else(|| "user_reject".into()),
        })
        .await
        .map_err(|_| "session thread closed".into())
}

#[tauri::command]
pub async fn cancel_file(state: State<'_, SessionState>, id: String) -> Result<(), String> {
    cmd_tx(&state)?
        .send(Cmd::CancelFile { id })
        .await
        .map_err(|_| "session thread closed".into())
}

/// Diagnostic-only: lets the frontend write to the file log so we can
/// confirm event delivery without DevTools. Newlines are escaped to keep
/// log parsing line-oriented.
#[tauri::command]
pub fn debug_log(msg: String) {
    let safe = msg.replace('\n', "\\n").replace('\r', "\\r");
    tracing::info!("FE: {safe}");
}

#[tauri::command]
pub async fn close_session(state: State<'_, SessionState>) -> Result<(), String> {
    let tx_opt = {
        let mut guard = state.handle.lock().unwrap();
        guard.take().map(|h| h.cmd_tx)
    };
    if let Some(tx) = tx_opt {
        let _ = tx.send(Cmd::Close).await;
    }
    Ok(())
}

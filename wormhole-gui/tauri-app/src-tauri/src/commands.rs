//! Tauri IPC commands. Architecture §6.1.

use crate::bridge::{start_event_pump, SessionState};
use crate::config::{self, Config, ConfigState};
use serde::Deserialize;
use std::path::PathBuf;
use tauri::{AppHandle, State};
use wormhole_gui_core::{spawn_session_thread, transfer, Cmd, CoreError, Role, SessionConfig};

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
    config: State<'_, ConfigState>,
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
    let mailbox_relay = config.mailbox_relay();
    let transit_relay = config.transit_relay();
    let lang = config.language();
    // Pre-validate the transit relay format so the user gets an immediate
    // localised toast instead of discovering the typo only when they try to
    // send their first file after a successful PAKE.
    transfer::parse_transit_relay(&transit_relay).map_err(|e| e.localize(&lang))?;
    let handle = spawn_session_thread(
        role,
        SessionConfig {
            mailbox_relay,
            transit_relay,
            numeric_code: config.numeric_code(),
            language: lang.clone(),
        },
    );
    let evt_rx = handle.evt_rx.clone();

    if let SessionMode::Recv = mode {
        let code_str = code.ok_or_else(|| "code required for recv mode".to_string())?;
        // Bypass magic-wormhole's zxcvbn check on Code::from_str: 6-digit
        // numeric codes (now the default) sometimes fall under the 16-bit
        // entropy threshold for sequence-like values (e.g. "123-456"). Our
        // 5-min TTL + relay-side PAKE handle the actual integrity check.
        let (np, pw) = code_str
            .split_once('-')
            .ok_or_else(|| CoreError::ShortCodeMissingDash.localize(&lang))?;
        if np.is_empty() || pw.is_empty() {
            return Err(CoreError::ShortCodeEmptyPart.localize(&lang));
        }
        #[allow(unsafe_code)]
        let parsed = unsafe {
            magic_wormhole::Code::from_components(
                magic_wormhole::Nameplate::new_unchecked(np),
                magic_wormhole::Password::new_unchecked(pw),
            )
        };
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
        .send(Cmd::SendFile {
            path: PathBuf::from(path),
        })
        .await
        .map_err(|_| "session thread closed".into())
}

#[tauri::command]
pub async fn accept_file(
    state: State<'_, SessionState>,
    config: State<'_, ConfigState>,
    id: String,
) -> Result<(), String> {
    let save_dir = config.download_dir();
    cmd_tx(&state)?
        .send(Cmd::AcceptFile { id, save_dir })
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

/// Open Windows Explorer with the given file selected. v0.1 is Windows-only;
/// extend with `cfg(target_os = ...)` when adding macOS/Linux.
#[tauri::command]
pub fn reveal_in_folder(path: String) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let p = std::path::Path::new(&path);
    if p.exists() {
        // raw_arg bypasses Rust's automatic quoting so explorer can parse
        // `/select,"<path>"` correctly even when <path> contains spaces or
        // non-ASCII chars. Without this, explorer falls back to Documents.
        std::process::Command::new("explorer")
            .raw_arg(format!("/select,\"{}\"", path))
            .spawn()
            .map_err(|e| format!("explorer: {e}"))?;
    } else if let Some(parent) = p.parent().filter(|d| d.exists()) {
        // File was moved/deleted after download — open the parent directory
        // instead of letting explorer fall back to Documents.
        std::process::Command::new("explorer")
            .arg(parent)
            .spawn()
            .map_err(|e| format!("explorer: {e}"))?;
    } else {
        return Err(format!("路径不存在: {path}"));
    }
    Ok(())
}

#[tauri::command]
pub fn get_config(config: State<'_, ConfigState>) -> Config {
    config.snapshot()
}

#[tauri::command]
pub fn set_config(config: State<'_, ConfigState>, mut new_config: Config) -> Result<(), String> {
    // Auto-accept is disabled in this build regardless of frontend / on-disk
    // value — defense in depth against a tampered config.json or bypassed UI.
    new_config.auto_accept = false;
    // The schema version is owned by the backend, not the frontend; if a
    // stale UI sends an older version, persist as the current schema so
    // serde defaults stay authoritative for fields the UI doesn't know about.
    new_config.version = config::SCHEMA_VERSION;
    config::save(&new_config).map_err(|e| format!("config save: {e}"))?;
    config.replace(new_config);
    Ok(())
}

/// Pick a directory using the OS file dialog. Returns None on cancel.
#[tauri::command]
pub async fn pick_download_dir(app: AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = std::sync::mpsc::channel();
    app.dialog().file().pick_folder(move |folder| {
        let _ = tx.send(folder);
    });
    let folder = tokio::task::spawn_blocking(move || rx.recv().ok().flatten())
        .await
        .map_err(|e| format!("dialog: {e}"))?;
    Ok(folder.map(|f| f.to_string()))
}

/// X-button path: end the session if any, give the mailbox 200ms to flush
/// the Bye, then destroy the window so the OS sees the process exit.
#[tauri::command]
pub async fn end_and_close(
    state: State<'_, SessionState>,
    window: tauri::Window,
) -> Result<(), String> {
    let tx_opt = {
        let mut guard = state.handle.lock().unwrap();
        guard.take().map(|h| h.cmd_tx)
    };
    if let Some(tx) = tx_opt {
        let _ = tx.send(Cmd::Close).await;
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
    let _ = window.destroy();
    Ok(())
}

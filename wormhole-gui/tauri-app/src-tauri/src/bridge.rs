//! Holds the active SessionHandle behind a Mutex so Tauri commands can
//! address it. Spawns an event-pump task that forwards Evt → Tauri events.
//!
//! Validated end-to-end by spike T1.13 (wormhole-spike/src/bridge.rs).

use std::sync::Mutex;
use tauri::{AppHandle, Emitter};
use wormhole_gui_core::{Evt, SessionHandle};

pub struct SessionState {
    pub handle: Mutex<Option<SessionHandle>>,
}

impl SessionState {
    pub fn new() -> Self {
        Self {
            handle: Mutex::new(None),
        }
    }
}

/// Spawn a task that pumps Evt from the session thread to the frontend.
pub fn start_event_pump(app: AppHandle, evt_rx: async_channel::Receiver<Evt>) {
    tauri::async_runtime::spawn(async move {
        while let Ok(evt) = evt_rx.recv().await {
            if let Err(e) = emit_evt(&app, evt) {
                tracing::warn!("emit failed: {e}");
            }
        }
        tracing::debug!("event pump exiting");
    });
}

fn emit_evt(app: &AppHandle, evt: Evt) -> tauri::Result<()> {
    match evt {
        Evt::Code(code) => app.emit("session:code", serde_json::json!({ "code": code })),
        Evt::SasReady { sas } => {
            app.emit("session:sas_ready", serde_json::json!({ "sas": sas }))
        }
        Evt::Connected => app.emit("session:connected", serde_json::json!({})),
        Evt::TextReceived { id, content, ts } => app.emit(
            "msg:text",
            serde_json::json!({ "id": id, "content": content, "ts": ts, "from": "peer" }),
        ),
        Evt::TextSent { id, content, ts } => app.emit(
            "msg:text_sent",
            serde_json::json!({ "id": id, "content": content, "ts": ts }),
        ),
        Evt::Closed { reason } => {
            app.emit("session:closed", serde_json::json!({ "reason": reason }))
        }
        Evt::Error { code, message } => app.emit(
            "error",
            serde_json::json!({ "code": code, "message": message }),
        ),
    }
}

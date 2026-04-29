//! Holds the active SessionHandle behind a Mutex so Tauri commands can
//! address it. Spawns an event-pump task that forwards Evt → Tauri events.

use std::sync::Mutex;
use tauri::{AppHandle, Emitter};
use wormhole_gui_core::{session::Direction, Evt, SessionHandle};

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

fn dir_str(d: Direction) -> &'static str {
    match d {
        Direction::In => "in",
        Direction::Out => "out",
    }
}

fn emit_evt(app: &AppHandle, evt: Evt) -> tauri::Result<()> {
    use serde_json::json;
    tracing::debug!("emit_evt: {evt:?}");
    match evt {
        Evt::Code(code) => app.emit("session:code", json!({ "code": code })),
        Evt::Connected => app.emit("session:connected", json!({})),
        Evt::TextReceived { id, content, ts } => app.emit(
            "msg:text",
            json!({ "id": id, "content": content, "ts": ts, "from": "peer" }),
        ),
        Evt::TextSent { id, content, ts } => app.emit(
            "msg:text_sent",
            json!({ "id": id, "content": content, "ts": ts }),
        ),
        Evt::FileOffer { id, name, size, mime } => app.emit(
            "msg:file_offer",
            json!({ "id": id, "name": name, "size": size, "mime": mime, "from": "peer" }),
        ),
        Evt::FileOfferSent { id, name, size } => app.emit(
            "msg:file_offer_sent",
            json!({ "id": id, "name": name, "size": size }),
        ),
        Evt::FileAccepted { id } => app.emit("file:accepted", json!({ "id": id })),
        Evt::FileProgress { id, bytes, total, dir } => app.emit(
            "file:progress",
            json!({ "id": id, "bytes": bytes, "total": total, "dir": dir_str(dir) }),
        ),
        Evt::FileDone { id, ok, dir, save_path } => app.emit(
            "file:done",
            json!({ "id": id, "ok": ok, "dir": dir_str(dir), "save_path": save_path }),
        ),
        Evt::FileCancelled { id, by } => {
            let by_str = match by {
                wormhole_gui_core::session::Cancelled::Local => "self",
                wormhole_gui_core::session::Cancelled::Peer => "peer",
            };
            app.emit("file:cancelled", json!({ "id": id, "by": by_str }))
        }
        Evt::FileError { id, message } => app.emit(
            "file:error",
            json!({ "id": id, "message": message }),
        ),
        Evt::Closed { reason } => {
            app.emit("session:closed", json!({ "reason": reason }))
        }
        Evt::Error { code, message } => app.emit(
            "error",
            json!({ "code": code, "message": message }),
        ),
    }
}

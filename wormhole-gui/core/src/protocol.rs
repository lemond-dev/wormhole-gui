//! Application-layer protocol carried over the magic-wormhole mailbox.
//!
//! All messages are JSON-encoded and bounded to the v0.1 size cap (1 MB
//! client-side; T1.5 verified server breaks above ~4 MB). The version field
//! is strictly checked; unknown variants are rejected.

use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 1;

/// Maximum allowed size of a single mailbox payload (JSON-serialized bytes).
/// Server-side limit observed in T1.5 is between 4 MB and 16 MB; we cap
/// well below that to keep wormhole alive.
pub const MAX_MAILBOX_PAYLOAD: usize = 1 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AppMsg {
    /// Both sides emit this once their local user has confirmed the SAS.
    SasOk { v: u32 },
    /// Either side may emit this if the SAS didn't match. Other side must close.
    SasReject { v: u32, reason: String },
    /// A chat-style text message.
    Text {
        v: u32,
        id: String,
        content: String,
        ts: u64,
    },
    /// Sender is offering a file. Receiver responds with FileAccept or FileReject.
    FileOffer {
        v: u32,
        id: String,
        name: String,
        size: u64,
        mime: Option<String>,
    },
    FileAccept { v: u32, id: String },
    FileReject { v: u32, id: String, reason: String },
    /// Sender attaches its transit hints + abilities for this offer.
    /// Receiver echoes its own.
    TransitHints {
        v: u32,
        id: String,
        // The hints/abilities are opaque JSON value here; the transfer
        // layer (transfer.rs) deserializes into magic_wormhole::transit
        // types when needed.
        hints: serde_json::Value,
        abilities: serde_json::Value,
    },
    /// Receiver acknowledges a successfully received file by id.
    FileDone { v: u32, id: String, ok: bool },
    /// Either side cancels an in-flight file by id.
    FileCancel { v: u32, id: String },
    Ping { v: u32 },
    Bye { v: u32 },
}

impl AppMsg {
    pub fn version(&self) -> u32 {
        match self {
            AppMsg::SasOk { v } => *v,
            AppMsg::SasReject { v, .. } => *v,
            AppMsg::Text { v, .. } => *v,
            AppMsg::FileOffer { v, .. } => *v,
            AppMsg::FileAccept { v, .. } => *v,
            AppMsg::FileReject { v, .. } => *v,
            AppMsg::TransitHints { v, .. } => *v,
            AppMsg::FileDone { v, .. } => *v,
            AppMsg::FileCancel { v, .. } => *v,
            AppMsg::Ping { v } => *v,
            AppMsg::Bye { v } => *v,
        }
    }

    pub fn check_version(&self) -> Result<(), crate::CoreError> {
        if self.version() != PROTOCOL_VERSION {
            return Err(crate::CoreError::Protocol(format!(
                "unsupported protocol version: got {}, want {}",
                self.version(),
                PROTOCOL_VERSION
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_roundtrip() {
        let m = AppMsg::Text {
            v: 1,
            id: "x".into(),
            content: "你好 🌍".into(),
            ts: 1234,
        };
        let j = serde_json::to_string(&m).unwrap();
        let back: AppMsg = serde_json::from_str(&j).unwrap();
        match back {
            AppMsg::Text { content, .. } => assert_eq!(content, "你好 🌍"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn version_check_passes_v1() {
        let m = AppMsg::Ping { v: 1 };
        assert!(m.check_version().is_ok());
    }

    #[test]
    fn version_check_rejects_unknown() {
        let m = AppMsg::Ping { v: 999 };
        assert!(m.check_version().is_err());
    }

    #[test]
    fn unknown_variant_is_rejected() {
        let bad = r#"{"type":"new_kind_we_dont_know","v":1}"#;
        let r: Result<AppMsg, _> = serde_json::from_str(bad);
        assert!(r.is_err());
    }
}

//! Application-layer protocol carried over the magic-wormhole mailbox.
//!
//! All messages are JSON-encoded and bounded to the v0.1 size cap (1 MB
//! client-side; T1.5 verified server breaks above ~4 MB). The version field
//! is strictly checked; unknown variants are rejected.
//!
//! File transfer protocol over mailbox (Phase 3):
//!
//!   sender   →   receiver:   FileOffer{id, name, size, mime, hints, abilities}
//!   receiver →   sender:     FileAccept{id, hints, abilities}
//!                          OR FileReject{id, reason}
//!   sender   →   receiver:   FileCancel{id}      (during transit, optional)
//!   receiver →   sender:     FileCancel{id}      (during transit, optional)
//!   receiver →   sender:     FileDone{id, ok}    (after transit completes)
//!
//! Bytes themselves go over `magic_wormhole::transit`, not the mailbox.

use magic_wormhole::transit::{Abilities, Hints};
use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 1;

/// Maximum size of a single mailbox payload (JSON bytes). Server-side limit
/// is between 4 MB and 16 MB (T1.5); we cap well below.
pub const MAX_MAILBOX_PAYLOAD: usize = 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AppMsg {
    // ─── Chat ───
    Text {
        v: u32,
        id: String,
        content: String,
        ts: u64,
    },

    // ─── File transfer (transit hints inlined) ───
    FileOffer {
        v: u32,
        id: String,
        name: String,
        size: u64,
        mime: Option<String>,
        hints: Hints,
        abilities: Abilities,
    },
    FileAccept {
        v: u32,
        id: String,
        hints: Hints,
        abilities: Abilities,
    },
    FileReject {
        v: u32,
        id: String,
        reason: String,
    },
    FileCancel {
        v: u32,
        id: String,
    },
    FileDone {
        v: u32,
        id: String,
        ok: bool,
    },

    // ─── Misc ───
    Ping {
        v: u32,
    },
    Bye {
        v: u32,
    },
}

impl AppMsg {
    pub fn version(&self) -> u32 {
        match self {
            AppMsg::Text { v, .. }
            | AppMsg::FileOffer { v, .. }
            | AppMsg::FileAccept { v, .. }
            | AppMsg::FileReject { v, .. }
            | AppMsg::FileCancel { v, .. }
            | AppMsg::FileDone { v, .. }
            | AppMsg::Ping { v }
            | AppMsg::Bye { v } => *v,
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

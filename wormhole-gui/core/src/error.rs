use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("wormhole error: {0}")]
    Wormhole(#[from] magic_wormhole::WormholeError),
    #[error("invalid code: {0}")]
    InvalidCode(String),
    #[error("PAKE failed (code wrong, or attacker)")]
    PakeFailed,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("protocol error: {0}")]
    Protocol(String),
    #[error("session not in a valid state for this operation")]
    InvalidState,
    #[error("internal channel closed")]
    ChannelClosed,
    #[error("{0}")]
    Other(String),
}

impl CoreError {
    pub fn code(&self) -> &'static str {
        match self {
            CoreError::Wormhole(_) => "wormhole",
            CoreError::InvalidCode(_) => "invalid_code",
            CoreError::PakeFailed => "pake_failed",
            CoreError::Io(_) => "io",
            CoreError::Json(_) => "json",
            CoreError::Protocol(_) => "protocol",
            CoreError::InvalidState => "invalid_state",
            CoreError::ChannelClosed => "channel_closed",
            CoreError::Other(_) => "other",
        }
    }
}

impl serde::Serialize for CoreError {
    fn serialize<S: serde::Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = ser.serialize_struct("CoreError", 2)?;
        s.serialize_field("code", self.code())?;
        s.serialize_field("message", &self.to_string())?;
        s.end()
    }
}

use thiserror::Error;

/// All errors that escape the core session loop to the UI layer.
///
/// `Display` produces an English message — that's the developer-facing
/// representation used by logs and `Debug`. For user-facing strings, call
/// [`CoreError::localize`] with the session's current UI language (the
/// session captures this at spawn time so a mid-session language switch
/// doesn't surprise an in-flight error message).
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
    #[error("peer timeout (heartbeat missed)")]
    PeerTimeout,
    #[error("folder transfer not supported; pick files individually")]
    FolderUnsupported,
    #[error("transit relay is empty")]
    TransitRelayEmpty,
    #[error("transit relay should not include the {0} prefix; expected host:port")]
    TransitRelayBadScheme(String),
    #[error("transit relay IPv6 literal is missing closing bracket: {0}")]
    TransitRelayIpv6MissingBracket(String),
    #[error("transit relay IPv6 literal is missing port: {0}")]
    TransitRelayIpv6MissingPort(String),
    #[error("transit relay format error (expected host:port): {0}")]
    TransitRelayBadFormat(String),
    #[error("transit relay port is invalid: {0}")]
    TransitRelayBadPort(String),
    #[error("transit relay host is empty")]
    TransitRelayHostEmpty,
    #[error("short code is missing the '-' separator")]
    ShortCodeMissingDash,
    #[error("short code has an empty nameplate or password section")]
    ShortCodeEmptyPart,
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
            CoreError::PeerTimeout => "peer_timeout",
            CoreError::FolderUnsupported => "folder_unsupported",
            CoreError::TransitRelayEmpty => "transit_relay_empty",
            CoreError::TransitRelayBadScheme(_) => "transit_relay_bad_scheme",
            CoreError::TransitRelayIpv6MissingBracket(_) => "transit_relay_ipv6_bracket",
            CoreError::TransitRelayIpv6MissingPort(_) => "transit_relay_ipv6_port",
            CoreError::TransitRelayBadFormat(_) => "transit_relay_bad_format",
            CoreError::TransitRelayBadPort(_) => "transit_relay_bad_port",
            CoreError::TransitRelayHostEmpty => "transit_relay_host_empty",
            CoreError::ShortCodeMissingDash => "short_code_missing_dash",
            CoreError::ShortCodeEmptyPart => "short_code_empty_part",
            CoreError::Other(_) => "other",
        }
    }

    /// Returns a localized user-facing message in the session's UI language.
    ///
    /// `lang` is the BCP-47-ish two-letter code from `Config.language`;
    /// unknown values fall through to English.
    ///
    /// Wrapped third-party errors (`Wormhole`, `Io`, `Json`) keep their
    /// upstream message verbatim — those crates produce English strings
    /// only, and translating them would mean shipping a dictionary that
    /// covers every error path in async-tungstenite, tungstenite,
    /// magic-wormhole, etc. We localise only the prefix.
    pub fn localize(&self, lang: &str) -> String {
        match lang {
            "zh" => self.zh(),
            _ => self.to_string(),
        }
    }

    fn zh(&self) -> String {
        match self {
            CoreError::Wormhole(e) => format!("Wormhole 错误：{e}"),
            CoreError::InvalidCode(s) => format!("无效短码：{s}"),
            CoreError::PakeFailed => "PAKE 失败（短码错误，或遭到攻击）".into(),
            CoreError::Io(e) => format!("IO 错误：{e}"),
            CoreError::Json(e) => format!("JSON 错误：{e}"),
            CoreError::Protocol(s) => format!("协议错误：{s}"),
            CoreError::InvalidState => "当前会话状态不允许此操作".into(),
            CoreError::ChannelClosed => "内部通道已关闭".into(),
            CoreError::PeerTimeout => "对方失联（心跳超时）".into(),
            CoreError::FolderUnsupported => "暂不支持发送文件夹，请逐个选择文件".into(),
            CoreError::TransitRelayEmpty => "transit relay 为空".into(),
            CoreError::TransitRelayBadScheme(s) => {
                format!("transit relay 不应带 {s} 前缀，应为 host:port 形式")
            }
            CoreError::TransitRelayIpv6MissingBracket(s) => {
                format!("transit relay IPv6 缺少右括号: {s}")
            }
            CoreError::TransitRelayIpv6MissingPort(s) => {
                format!("transit relay IPv6 缺少端口: {s}")
            }
            CoreError::TransitRelayBadFormat(s) => {
                format!("transit relay 格式错误 (应为 host:port): {s}")
            }
            CoreError::TransitRelayBadPort(s) => format!("transit relay 端口无效: {s}"),
            CoreError::TransitRelayHostEmpty => "transit relay host 为空".into(),
            CoreError::ShortCodeMissingDash => "短码格式错误：缺少 '-' 分隔".into(),
            CoreError::ShortCodeEmptyPart => "短码格式错误：nameplate 或 password 为空".into(),
            CoreError::Other(s) => s.clone(),
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

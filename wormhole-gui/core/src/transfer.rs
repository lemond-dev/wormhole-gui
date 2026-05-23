//! File transfer over `magic_wormhole::transit`.
//!
//! Provides:
//! - `init_connector` — sets up a TransitConnector + our hints/abilities
//! - `derive_transit_key` — pure helper
//! - `stream_send` — read file → write 16KB records to transit, with
//!   throttled progress callback
//! - `stream_recv` — read records → append to file, with throttled progress
//!
//! Bytes hash is *not* verified end-to-end here in v0.1; transit's per-record
//! Noise MAC is sufficient for integrity. v0.2 may add an explicit SHA-256.

use crate::CoreError;
use futures::io::{AsyncReadExt, AsyncWriteExt};
use magic_wormhole::{
    transit::{
        self, Abilities, DirectHint, Hints, RelayHint, Transit, TransitConnector, TransitKey,
        TransitRole,
    },
    AppID, Wormhole,
};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// Build a transit connector with our hints/abilities. `transit_relay` is
/// the user-configured `host:port` (an optional `tcp:` / `tcp://` prefix is
/// tolerated so the value the Settings UI shows can be pasted back as-is).
pub async fn init_connector(transit_relay: &str) -> Result<TransitConnector, CoreError> {
    let (host, port) = parse_transit_relay(transit_relay)?;
    let abilities = Abilities::ALL;
    let relay = RelayHint::new(None, [DirectHint::new(host, port)], []);
    let connector = transit::init(abilities, None, vec![relay])
        .await
        .map_err(CoreError::Io)?;
    Ok(connector)
}

/// Parse `[tcp[://]]host:port`. Public so callers (e.g. `start_session`) can
/// pre-validate the user-supplied value at session-start time instead of
/// only discovering a typo when the first file transfer is attempted.
///
/// Accepts:
/// - `host:port` — `relay.example.com:4001`
/// - `tcp:host:port` / `tcp://host:port` — UI sometimes displays this form
/// - `[v6addr]:port` — bracketed IPv6 literal
///
/// Explicitly rejects `ws://` / `wss://` / `http(s)://` since those belong
/// in the *mailbox* field; tolerating them silently here would let a misfile
/// pass session start and only blow up on the first file send.
pub fn parse_transit_relay(raw: &str) -> Result<(String, u16), CoreError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(CoreError::Other("transit relay 为空".into()));
    }
    // Reject schemes that belong to the mailbox field — the most common
    // user error is pasting the mailbox URL into the wrong box.
    for bad in ["ws://", "wss://", "http://", "https://"] {
        if trimmed.starts_with(bad) {
            return Err(CoreError::Other(format!(
                "transit relay 不应带 {bad} 前缀，应为 host:port 形式"
            )));
        }
    }
    // Strip the only supported optional scheme.
    let s = trimmed
        .strip_prefix("tcp://")
        .or_else(|| trimmed.strip_prefix("tcp:"))
        .unwrap_or(trimmed);

    // IPv6 literal: `[addr]:port`.
    let (host, port_str) = if let Some(rest) = s.strip_prefix('[') {
        let (addr, tail) = rest.rsplit_once(']').ok_or_else(|| {
            CoreError::Other(format!("transit relay IPv6 缺少右括号: {raw}"))
        })?;
        let port_part = tail
            .strip_prefix(':')
            .ok_or_else(|| CoreError::Other(format!("transit relay IPv6 缺少端口: {raw}")))?;
        (addr, port_part)
    } else {
        s.rsplit_once(':').ok_or_else(|| {
            CoreError::Other(format!("transit relay 格式错误 (应为 host:port): {raw}"))
        })?
    };
    let port = port_str
        .parse::<u16>()
        .map_err(|_| CoreError::Other(format!("transit relay 端口无效: {port_str}")))?;
    if host.is_empty() {
        return Err(CoreError::Other("transit relay host 为空".into()));
    }
    Ok((host.to_string(), port))
}

#[cfg(test)]
mod tests {
    use super::parse_transit_relay;

    #[test]
    fn plain_host_port() {
        let (h, p) = parse_transit_relay("example.com:4001").unwrap();
        assert_eq!(h, "example.com");
        assert_eq!(p, 4001);
    }

    #[test]
    fn strips_tcp_scheme() {
        let (h, p) = parse_transit_relay("tcp://example.com:4001").unwrap();
        assert_eq!(h, "example.com");
        assert_eq!(p, 4001);
        let (h, p) = parse_transit_relay("tcp:example.com:4001").unwrap();
        assert_eq!(h, "example.com");
        assert_eq!(p, 4001);
    }

    #[test]
    fn ipv6_literal() {
        let (h, p) = parse_transit_relay("[::1]:4001").unwrap();
        assert_eq!(h, "::1");
        assert_eq!(p, 4001);
    }

    #[test]
    fn rejects_mailbox_scheme() {
        for bad in [
            "wss://mailbox.example/v1",
            "ws://example:4000/v1",
            "http://x:1",
        ] {
            assert!(
                parse_transit_relay(bad).is_err(),
                "should reject {bad} as transit"
            );
        }
    }

    #[test]
    fn rejects_missing_port_or_empty() {
        assert!(parse_transit_relay("example.com").is_err());
        assert!(parse_transit_relay(":4001").is_err());
        assert!(parse_transit_relay("example.com:99999").is_err());
        assert!(parse_transit_relay("").is_err());
        assert!(parse_transit_relay("  ").is_err());
    }
}

pub fn our_hints(connector: &TransitConnector) -> Hints {
    (**connector.our_hints()).clone()
}

pub fn our_abilities(connector: &TransitConnector) -> Abilities {
    *connector.our_abilities()
}

/// Derive the transit subkey from a Wormhole's session key.
/// Mirrors the (pub(crate)) derivation in magic-wormhole's source.
pub fn derive_transit_key(wh: &Wormhole) -> magic_wormhole::Key<TransitKey> {
    let appid: &AppID = wh.appid();
    let purpose = format!("{}/transit-key", appid.as_ref());
    wh.key().derive_subkey_from_purpose::<TransitKey>(&purpose)
}

/// Connect a transit. Caller picks role (Leader for sender, Follower for
/// receiver).
pub async fn connect_transit(
    connector: TransitConnector,
    role: TransitRole,
    transit_key: magic_wormhole::Key<TransitKey>,
    their_abilities: Abilities,
    their_hints: Hints,
) -> Result<Transit, CoreError> {
    let (transit, _info) = connector
        .connect(role, transit_key, their_abilities, Arc::new(their_hints))
        .await
        .map_err(|e| CoreError::Other(format!("transit connect: {e}")))?;
    tracing::info!("transit established");
    Ok(transit)
}

/// Read a local file and stream its bytes to the peer over `transit`.
/// Calls `on_progress(bytes_sent)` at most every ~100ms; final call always
/// fires when the transfer finishes successfully.
///
/// `cancel`: future that resolves when the user cancels — leads to a clean
/// abort (drops the transit, returns Cancelled).
pub async fn stream_send<F>(
    transit: &mut Transit,
    file_path: &Path,
    expected_size: u64,
    mut on_progress: F,
    cancel: async_channel::Receiver<()>,
) -> Result<(), CoreError>
where
    F: FnMut(u64) + Send,
{
    let mut f = smol::fs::File::open(file_path).await?;
    let mut buf = vec![0u8; 16 * 1024];
    let mut sent: u64 = 0;
    let mut last_emit = std::time::Instant::now();
    on_progress(0);
    loop {
        if let Ok(()) = cancel.try_recv() {
            return Err(CoreError::Other("cancelled by user".into()));
        }
        let n = f.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        transit
            .send_record(&buf[..n])
            .await
            .map_err(|e| CoreError::Other(format!("send_record: {e}")))?;
        sent += n as u64;
        if last_emit.elapsed() >= Duration::from_millis(100) {
            on_progress(sent);
            last_emit = std::time::Instant::now();
        }
    }
    transit
        .flush()
        .await
        .map_err(|e| CoreError::Other(format!("transit flush: {e}")))?;
    on_progress(sent);
    if sent != expected_size {
        return Err(CoreError::Protocol(format!(
            "size mismatch: streamed {sent}, declared {expected_size}"
        )));
    }
    Ok(())
}

/// Receive bytes from `transit`, write to `save_path` until `expected_size`
/// has been received. Same throttling and cancellation behavior as
/// `stream_send`.
pub async fn stream_recv<F>(
    transit: &mut Transit,
    save_path: &Path,
    expected_size: u64,
    mut on_progress: F,
    cancel: async_channel::Receiver<()>,
) -> Result<(), CoreError>
where
    F: FnMut(u64) + Send,
{
    if let Some(parent) = save_path.parent() {
        let _ = smol::fs::create_dir_all(parent).await;
    }
    let mut f = smol::fs::File::create(save_path).await?;
    let mut got: u64 = 0;
    let mut last_emit = std::time::Instant::now();
    on_progress(0);

    while got < expected_size {
        if let Ok(()) = cancel.try_recv() {
            // Drop the partial file on cancel.
            drop(f);
            let _ = smol::fs::remove_file(save_path).await;
            return Err(CoreError::Other("cancelled by user".into()));
        }
        let buf = transit
            .receive_record()
            .await
            .map_err(|e| CoreError::Other(format!("receive_record: {e}")))?;
        let remaining = (expected_size - got) as usize;
        if buf.len() > remaining {
            // Sender overshot: refuse the extra bytes.
            f.write_all(&buf[..remaining]).await?;
            got += remaining as u64;
            on_progress(got);
            return Err(CoreError::Protocol(format!(
                "sender overshot: declared {expected_size}, wrote at least {}",
                got + (buf.len() - remaining) as u64
            )));
        }
        f.write_all(&buf).await?;
        got += buf.len() as u64;
        if last_emit.elapsed() >= Duration::from_millis(100) {
            on_progress(got);
            last_emit = std::time::Instant::now();
        }
    }
    f.flush().await?;
    f.close().await?;
    on_progress(got);
    Ok(())
}

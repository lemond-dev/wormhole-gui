//! Session loop: runs on a dedicated OS thread under `smol::block_on`,
//! exposes its API as async-channel `Cmd` (in) / `Evt` (out).
//!
//! v0.1 phase 0/1: covers session lifecycle (allocate code → PAKE → SAS →
//! send/recv text). File transfer (Phase 3) extends this with TransitHints
//! handling and a transfer module.

use crate::{protocol::*, CoreError};
use async_channel::{bounded, Receiver, Sender};
use magic_wormhole::{transfer, Code, MailboxConnection, Wormhole};
use std::time::SystemTime;

#[derive(Debug, Clone, Copy)]
pub enum Role {
    Allocator,
    Joiner,
}

#[derive(Debug)]
pub enum Cmd {
    /// Joiner only: peer-supplied code (parsed externally for early validation).
    JoinCode(Code),
    /// Local user has confirmed the SAS verification.
    ConfirmSas { matches: bool },
    /// Send a text message.
    SendText(String),
    /// Close the session gracefully.
    Close,
}

#[derive(Debug, Clone)]
pub enum Evt {
    /// Allocator: short code is now ready to display.
    Code(String),
    /// PAKE finished; SAS is ready for both users to confirm.
    SasReady { sas: String },
    /// Both users confirmed; the session is fully connected.
    Connected,
    /// A text message arrived from peer.
    TextReceived { id: String, content: String, ts: u64 },
    /// Locally-sent message acknowledged (echoed back to UI for display).
    TextSent { id: String, content: String, ts: u64 },
    /// Session closed (gracefully or due to error).
    Closed { reason: String },
    /// Recoverable / informational error.
    Error { code: String, message: String },
}

pub struct SessionHandle {
    pub cmd_tx: Sender<Cmd>,
    pub evt_rx: Receiver<Evt>,
    pub thread: std::thread::JoinHandle<()>,
}

pub fn spawn_session_thread(role: Role) -> SessionHandle {
    let (cmd_tx, cmd_rx) = bounded::<Cmd>(16);
    let (evt_tx, evt_rx) = bounded::<Evt>(64);
    let thread = std::thread::Builder::new()
        .name("wh-session".into())
        .spawn(move || {
            smol::block_on(async move {
                let result = run(role, cmd_rx, evt_tx.clone()).await;
                let reason = match result {
                    Ok(()) => "ok".to_string(),
                    Err(e) => format!("{e}"),
                };
                let _ = evt_tx.send(Evt::Closed { reason }).await;
            });
        })
        .expect("spawn session thread");
    SessionHandle {
        cmd_tx,
        evt_rx,
        thread,
    }
}

async fn run(
    role: Role,
    cmd_rx: Receiver<Cmd>,
    evt_tx: Sender<Evt>,
) -> Result<(), CoreError> {
    // Phase 1 — establish mailbox + PAKE
    let cfg = transfer::APP_CONFIG.clone();
    let mut wh = match role {
        Role::Allocator => {
            let mc = MailboxConnection::create(cfg, 2).await?;
            let code = mc.code().to_string();
            evt_tx.send(Evt::Code(code)).await.map_err(|_| CoreError::ChannelClosed)?;
            Wormhole::connect(mc).await?
        }
        Role::Joiner => {
            let code = match cmd_rx.recv().await.map_err(|_| CoreError::ChannelClosed)? {
                Cmd::JoinCode(c) => c,
                other => {
                    return Err(CoreError::Protocol(format!(
                        "expected JoinCode, got {other:?}"
                    )))
                }
            };
            let mc = MailboxConnection::connect(cfg, code, true).await?;
            Wormhole::connect(mc).await.map_err(|e| match e {
                magic_wormhole::WormholeError::PakeFailed => CoreError::PakeFailed,
                other => CoreError::Wormhole(other),
            })?
        }
    };

    // Phase 2 — derive SAS from verifier (T1.13 verified equality cross-side)
    let sas = derive_sas(&wh);
    evt_tx
        .send(Evt::SasReady { sas: sas.clone() })
        .await
        .map_err(|_| CoreError::ChannelClosed)?;

    // Phase 3 — wait for both local confirmation and peer's sas_ok message
    let mut local_ok = false;
    let mut peer_ok = false;
    loop {
        if local_ok && peer_ok {
            break;
        }
        use futures::FutureExt;
        futures::select! {
            cmd = cmd_rx.recv().fuse() => match cmd.map_err(|_| CoreError::ChannelClosed)? {
                Cmd::ConfirmSas { matches: true } => {
                    wh.send_json(&AppMsg::SasOk { v: PROTOCOL_VERSION }).await?;
                    local_ok = true;
                }
                Cmd::ConfirmSas { matches: false } => {
                    wh.send_json(&AppMsg::SasReject { v: PROTOCOL_VERSION, reason: "user_mismatch".into() }).await?;
                    return Ok(()); // close session
                }
                Cmd::Close => return Ok(()),
                other => {
                    return Err(CoreError::Protocol(format!(
                        "command {other:?} not allowed in SasPending"
                    )));
                }
            },
            inbound = wh.receive_json::<AppMsg>().fuse() => {
                let msg = inbound??;
                msg.check_version()?;
                match msg {
                    AppMsg::SasOk { .. } => peer_ok = true,
                    AppMsg::SasReject { reason, .. } => {
                        return Err(CoreError::Other(format!("peer rejected SAS: {reason}")));
                    }
                    other => {
                        return Err(CoreError::Protocol(format!(
                            "peer sent {other:?} before sas_ok"
                        )));
                    }
                }
            }
        }
    }

    evt_tx
        .send(Evt::Connected)
        .await
        .map_err(|_| CoreError::ChannelClosed)?;

    // Phase 4 — connected event loop
    loop {
        use futures::FutureExt;
        futures::select! {
            cmd = cmd_rx.recv().fuse() => match cmd.map_err(|_| CoreError::ChannelClosed)? {
                Cmd::SendText(content) => {
                    if content.len() > MAX_MAILBOX_PAYLOAD / 2 {
                        evt_tx.send(Evt::Error {
                            code: "text_too_long".into(),
                            message: format!("text exceeds {} bytes; not yet supported in v0.1",
                                MAX_MAILBOX_PAYLOAD / 2),
                        }).await.ok();
                        continue;
                    }
                    let id = make_id();
                    let ts = now_ms();
                    let msg = AppMsg::Text { v: PROTOCOL_VERSION, id: id.clone(), content: content.clone(), ts };
                    wh.send_json(&msg).await?;
                    evt_tx.send(Evt::TextSent { id, content, ts }).await.ok();
                }
                Cmd::Close => {
                    let _ = wh.send_json(&AppMsg::Bye { v: PROTOCOL_VERSION }).await;
                    return Ok(());
                }
                Cmd::ConfirmSas { .. } => {
                    // already past SAS; ignore but log
                    tracing::warn!("ConfirmSas received in Connected state; ignored");
                }
                Cmd::JoinCode(_) => {
                    return Err(CoreError::InvalidState);
                }
            },
            inbound = wh.receive_json::<AppMsg>().fuse() => {
                let msg = inbound??;
                msg.check_version()?;
                match msg {
                    AppMsg::Text { id, content, ts, .. } => {
                        evt_tx.send(Evt::TextReceived { id, content, ts }).await.ok();
                    }
                    AppMsg::Ping { .. } => { /* ignore */ }
                    AppMsg::Bye { .. } => {
                        return Ok(());
                    }
                    AppMsg::SasOk { .. } | AppMsg::SasReject { .. } => {
                        // late SAS messages — ignore in Connected
                    }
                    other => {
                        // file ops not implemented in Phase 1; log and ignore
                        tracing::warn!("unhandled msg in v0.1 phase 1: {other:?}");
                    }
                }
            }
        }
    }
}

fn derive_sas(wh: &Wormhole) -> String {
    let v = wh.verifier();
    let bytes = v.as_slice();
    let n = u16::from_be_bytes([bytes[0], bytes[1]]);
    format!("{:04}", n % 10000)
}

fn make_id() -> String {
    let n = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{n:x}")
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

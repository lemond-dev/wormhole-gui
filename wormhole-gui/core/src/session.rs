//! Session loop: runs on a dedicated OS thread under `smol::block_on`,
//! exposes its API as async-channel `Cmd` (in) / `Evt` (out).
//!
//! Phase 3 adds file transfer over `transit`. The session task itself owns
//! the Wormhole and sequences mailbox messages; transit byte-streaming runs
//! in spawned smol tasks that report back via two internal channels:
//!
//!   1. `evt_tx` — relays progress / done events to the UI
//!   2. `outbox_tx` — asks the session loop to forward an `AppMsg` over the
//!      wormhole on the task's behalf (since only the loop owns the wormhole)

use crate::{protocol::*, storage, transfer, CoreError};
use async_channel::{bounded, Receiver, Sender};
use magic_wormhole::{
    transfer as mw_transfer, transit::TransitRole, Code, MailboxConnection, Wormhole,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

/// App-level heartbeat over the mailbox: the magic-wormhole relay is pubsub —
/// it doesn't tell us when a peer drops. We send Pings every HEARTBEAT_INTERVAL
/// and declare the peer dead if nothing has been received within PEER_TIMEOUT.
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
const PEER_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Copy)]
pub enum Role {
    Allocator,
    Joiner,
}

#[derive(Debug)]
pub enum Cmd {
    JoinCode(Code),
    SendText(String),
    SendFile { path: PathBuf },
    AcceptFile { id: String, save_dir: PathBuf },
    RejectFile { id: String, reason: String },
    CancelFile { id: String },
    Close,
}

#[derive(Debug, Clone)]
pub enum Evt {
    Code(String),
    Connected,
    TextReceived {
        id: String,
        content: String,
        ts: u64,
    },
    TextSent {
        id: String,
        content: String,
        ts: u64,
    },
    /// Peer sent us a file offer.
    FileOffer {
        id: String,
        name: String,
        size: u64,
        mime: Option<String>,
    },
    /// Local user's outgoing offer was sent to peer (not yet accepted).
    FileOfferSent {
        id: String,
        name: String,
        size: u64,
    },
    /// Peer accepted our offer; transit started.
    FileAccepted {
        id: String,
    },
    /// Streaming progress (in or out). `bytes` is cumulative.
    FileProgress {
        id: String,
        bytes: u64,
        total: u64,
        dir: Direction,
    },
    /// Transfer completed (either direction).
    FileDone {
        id: String,
        ok: bool,
        dir: Direction,
        save_path: Option<String>,
    },
    /// Transfer was cancelled by either side.
    FileCancelled {
        id: String,
        by: Cancelled,
    },
    /// Soft error specific to a file id (kept open).
    FileError {
        id: String,
        message: String,
    },
    Closed {
        reason: String,
    },
    Error {
        code: String,
        message: String,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum Direction {
    In,
    Out,
}

#[derive(Debug, Clone, Copy)]
pub enum Cancelled {
    Local,
    Peer,
}

pub struct SessionHandle {
    pub cmd_tx: Sender<Cmd>,
    pub evt_rx: Receiver<Evt>,
    pub thread: std::thread::JoinHandle<()>,
}

/// Configuration the session thread needs at startup. Pulled out into a
/// struct so future per-session knobs (timeouts, app_id override) don't
/// require yet another spawn-time positional argument.
#[derive(Debug, Clone)]
pub struct SessionConfig {
    pub mailbox_relay: String,
    pub transit_relay: String,
    pub numeric_code: bool,
}

pub fn spawn_session_thread(role: Role, cfg: SessionConfig) -> SessionHandle {
    let (cmd_tx, cmd_rx) = bounded::<Cmd>(32);
    let (evt_tx, evt_rx) = bounded::<Evt>(128);
    let thread = std::thread::Builder::new()
        .name("wh-session".into())
        .spawn(move || {
            tracing::info!(
                "session thread started role={role:?} numeric={} mailbox={} transit={}",
                cfg.numeric_code,
                cfg.mailbox_relay,
                cfg.transit_relay
            );
            smol::block_on(async move {
                let result = run(role, cfg, cmd_rx, evt_tx.clone()).await;
                let reason = match result {
                    Ok(()) => {
                        tracing::info!("session ended cleanly");
                        "ok".to_string()
                    }
                    Err(ref e) => {
                        tracing::error!("session ended with error: {e:?}");
                        format!("{e}")
                    }
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

// ============================================================
// Internal session state for in-flight transfers
// ============================================================

#[allow(dead_code)]
struct OutgoingPending {
    id: String,
    path: PathBuf,
    name: String,
    size: u64,
    /// Connector held until peer accepts; consumed when transit starts.
    connector: Option<magic_wormhole::transit::TransitConnector>,
    /// Sender → transit task; drop or send to abort.
    cancel_tx: Option<Sender<()>>,
}

#[allow(dead_code)]
struct IncomingPending {
    id: String,
    name: String,
    size: u64,
    mime: Option<String>,
    their_hints: magic_wormhole::transit::Hints,
    their_abilities: magic_wormhole::transit::Abilities,
    cancel_tx: Option<Sender<()>>,
}

// ============================================================
// run()
// ============================================================

async fn run(
    role: Role,
    session_cfg: SessionConfig,
    cmd_rx: Receiver<Cmd>,
    evt_tx: Sender<Evt>,
) -> Result<(), CoreError> {
    // ── PAKE ──
    let mut cfg = mw_transfer::APP_CONFIG.clone();
    cfg.rendezvous_url = std::borrow::Cow::Owned(session_cfg.mailbox_relay.clone());
    let transit_relay = session_cfg.transit_relay.clone();
    let numeric_code = session_cfg.numeric_code;
    tracing::info!("connecting to relay: {}", cfg.rendezvous_url);
    let mut wh = match role {
        Role::Allocator => {
            let mc = if numeric_code {
                let pw = generate_numeric_password(6);
                tracing::info!("allocator: creating mailbox with numeric password");
                // SAFETY: zxcvbn would reject 6-digit passwords, but our 5-min
                // TTL (Allocator screen) + 15s heartbeat already gate brute-force
                // attempts well below the 1M-combo space.
                #[allow(unsafe_code)]
                let password = unsafe { magic_wormhole::Password::new_unchecked(pw) };
                MailboxConnection::create_with_password(cfg, password)
                    .await
                    .map_err(|e| {
                        tracing::error!("MailboxConnection::create_with_password failed: {e:?}");
                        e
                    })?
            } else {
                tracing::info!("allocator: creating mailbox (wordlist code)");
                MailboxConnection::create(cfg, 2).await.map_err(|e| {
                    tracing::error!("MailboxConnection::create failed: {e:?}");
                    e
                })?
            };
            let code = mc.code().to_string();
            tracing::info!("allocator: mailbox created, code={code}");
            let _ = evt_tx.send(Evt::Code(code)).await;
            tracing::info!("allocator: waiting for peer (Wormhole::connect)");
            let wh = Wormhole::connect(mc).await.map_err(|e| {
                tracing::error!("Wormhole::connect (allocator) failed: {e:?}");
                e
            })?;
            tracing::info!("allocator: peer connected, PAKE done");
            wh
        }
        Role::Joiner => {
            tracing::info!("joiner: waiting for JoinCode cmd");
            let code = match cmd_rx.recv().await.map_err(|_| CoreError::ChannelClosed)? {
                Cmd::JoinCode(c) => c,
                other => {
                    return Err(CoreError::Protocol(format!(
                        "expected JoinCode, got {other:?}"
                    )))
                }
            };
            tracing::info!("joiner: connecting to mailbox with code");
            let mc = MailboxConnection::connect(cfg, code, true)
                .await
                .map_err(|e| {
                    tracing::error!("MailboxConnection::connect failed: {e:?}");
                    e
                })?;
            tracing::info!("joiner: mailbox connected, running PAKE");
            let wh = Wormhole::connect(mc).await.map_err(|e| {
                tracing::error!("Wormhole::connect (joiner) failed: {e:?}");
                match e {
                    magic_wormhole::WormholeError::PakeFailed => CoreError::PakeFailed,
                    other => CoreError::Wormhole(other),
                }
            })?;
            tracing::info!("joiner: PAKE done");
            wh
        }
    };

    // PAKE's verifier is matched implicitly; v0.1 skips explicit SAS confirmation.
    tracing::info!("PAKE done; skipping SAS handshake");
    let _ = evt_tx.send(Evt::Connected).await;

    // ── Connected event loop with file-transfer dispatch ──
    let (outbox_tx, outbox_rx) = bounded::<AppMsg>(64);
    let mut outgoing: HashMap<String, OutgoingPending> = HashMap::new();
    let mut incoming: HashMap<String, IncomingPending> = HashMap::new();
    let mut last_seen = Instant::now();

    loop {
        use futures::FutureExt;
        futures::select! {
            cmd = cmd_rx.recv().fuse() => {
                let c = cmd.map_err(|_| CoreError::ChannelClosed)?;
                if matches!(c, Cmd::Close) {
                    tracing::info!("close cmd received, sending Bye and shutting down");
                    let _ = wh.send_json(&AppMsg::Bye { v: PROTOCOL_VERSION }).await;
                    // Drop all in-flight cancel senders so transit tasks exit.
                    for (_, o) in outgoing.iter_mut() { o.cancel_tx = None; }
                    for (_, i) in incoming.iter_mut() { i.cancel_tx = None; }
                    return Ok(());
                }
                if let Err(e) = handle_local_cmd(c, &mut wh, &evt_tx, &outbox_tx, &mut outgoing, &mut incoming, &transit_relay).await {
                    tracing::error!("handle_local_cmd error: {e:?}");
                    let _ = evt_tx.send(Evt::Error {
                        code: e.code().into(),
                        message: format!("{e}"),
                    }).await;
                }
            },
            inbound = wh.receive_json::<AppMsg>().fuse() => {
                last_seen = Instant::now();
                let msg = inbound??;
                msg.check_version()?;
                if matches!(msg, AppMsg::Bye { .. }) {
                    return Ok(());
                }
                if let Err(e) = handle_peer_msg(msg, &wh, &evt_tx, &outbox_tx, &mut outgoing, &mut incoming).await {
                    tracing::error!("handle_peer_msg error: {e:?}");
                    let _ = evt_tx.send(Evt::Error {
                        code: e.code().into(),
                        message: format!("{e}"),
                    }).await;
                }
            },
            outbound = outbox_rx.recv().fuse() => {
                let m = outbound.map_err(|_| CoreError::ChannelClosed)?;
                wh.send_json(&m).await?;
            }
            _ = smol::Timer::after(HEARTBEAT_INTERVAL).fuse() => {
                if last_seen.elapsed() > PEER_TIMEOUT {
                    tracing::error!("peer timeout: no message in {:?}", PEER_TIMEOUT);
                    return Err(CoreError::Other("对方失联（心跳超时）".into()));
                }
                tracing::debug!("heartbeat ping");
                let _ = wh.send_json(&AppMsg::Ping { v: PROTOCOL_VERSION }).await;
            }
        }
    }
}

// ============================================================
// Handlers
// ============================================================

#[allow(clippy::too_many_arguments)]
async fn handle_local_cmd(
    cmd: Cmd,
    wh: &mut Wormhole,
    evt_tx: &Sender<Evt>,
    outbox_tx: &Sender<AppMsg>,
    outgoing: &mut HashMap<String, OutgoingPending>,
    incoming: &mut HashMap<String, IncomingPending>,
    transit_relay: &str,
) -> Result<(), CoreError> {
    tracing::info!("local cmd: {:?}", std::mem::discriminant(&cmd));
    match cmd {
        Cmd::SendText(content) => {
            if content.len() > MAX_MAILBOX_PAYLOAD / 2 {
                let _ = evt_tx
                    .send(Evt::Error {
                        code: "text_too_long".into(),
                        message: format!(
                            "text exceeds {} bytes; not yet supported in v0.1",
                            MAX_MAILBOX_PAYLOAD / 2
                        ),
                    })
                    .await;
                return Ok(());
            }
            let id = make_id();
            let ts = now_ms();
            let msg = AppMsg::Text {
                v: PROTOCOL_VERSION,
                id: id.clone(),
                content: content.clone(),
                ts,
            };
            wh.send_json(&msg).await?;
            let _ = evt_tx.send(Evt::TextSent { id, content, ts }).await;
        }
        Cmd::SendFile { path } => {
            send_file_offer(path, wh, evt_tx, outgoing, transit_relay).await?;
        }
        Cmd::AcceptFile { id, save_dir } => {
            accept_file(id, save_dir, wh, evt_tx, outbox_tx, incoming, transit_relay).await?;
        }
        Cmd::RejectFile { id, reason } => {
            if incoming.remove(&id).is_some() {
                let _ = wh
                    .send_json(&AppMsg::FileReject {
                        v: PROTOCOL_VERSION,
                        id: id.clone(),
                        reason,
                    })
                    .await;
                let _ = evt_tx
                    .send(Evt::FileCancelled {
                        id,
                        by: Cancelled::Local,
                    })
                    .await;
            }
        }
        Cmd::CancelFile { id } => {
            // Could be outgoing or incoming.
            if let Some(o) = outgoing.get_mut(&id) {
                if let Some(tx) = o.cancel_tx.take() {
                    let _ = tx.send(()).await;
                }
                let _ = wh
                    .send_json(&AppMsg::FileCancel {
                        v: PROTOCOL_VERSION,
                        id: id.clone(),
                    })
                    .await;
                outgoing.remove(&id);
                let _ = evt_tx
                    .send(Evt::FileCancelled {
                        id,
                        by: Cancelled::Local,
                    })
                    .await;
            } else if let Some(i) = incoming.get_mut(&id) {
                if let Some(tx) = i.cancel_tx.take() {
                    let _ = tx.send(()).await;
                }
                let _ = wh
                    .send_json(&AppMsg::FileCancel {
                        v: PROTOCOL_VERSION,
                        id: id.clone(),
                    })
                    .await;
                incoming.remove(&id);
                let _ = evt_tx
                    .send(Evt::FileCancelled {
                        id,
                        by: Cancelled::Local,
                    })
                    .await;
            }
        }
        Cmd::Close => unreachable!(),
        Cmd::JoinCode(_) => return Err(CoreError::InvalidState),
    }
    Ok(())
}

async fn handle_peer_msg(
    msg: AppMsg,
    wh: &Wormhole,
    evt_tx: &Sender<Evt>,
    outbox_tx: &Sender<AppMsg>,
    outgoing: &mut HashMap<String, OutgoingPending>,
    incoming: &mut HashMap<String, IncomingPending>,
) -> Result<(), CoreError> {
    tracing::info!("peer msg: {:?}", std::mem::discriminant(&msg));
    match msg {
        AppMsg::Text {
            id, content, ts, ..
        } => {
            let _ = evt_tx.send(Evt::TextReceived { id, content, ts }).await;
        }
        AppMsg::Ping { .. } | AppMsg::Bye { .. } => {}

        AppMsg::FileOffer {
            id,
            name,
            size,
            mime,
            hints,
            abilities,
            ..
        } => {
            // Surface to UI; user picks accept or reject.
            incoming.insert(
                id.clone(),
                IncomingPending {
                    id: id.clone(),
                    name: name.clone(),
                    size,
                    mime: mime.clone(),
                    their_hints: hints,
                    their_abilities: abilities,
                    cancel_tx: None,
                },
            );
            let _ = evt_tx
                .send(Evt::FileOffer {
                    id,
                    name,
                    size,
                    mime,
                })
                .await;
        }

        AppMsg::FileAccept {
            id,
            hints: their_hints,
            abilities: their_abilities,
            ..
        } => {
            // Peer accepted our outgoing offer; spawn the sender transit task.
            let pending = match outgoing.get_mut(&id) {
                Some(p) => p,
                None => return Ok(()), // stale; ignore
            };
            let connector = match pending.connector.take() {
                Some(c) => c,
                None => return Ok(()),
            };
            let (cancel_tx, cancel_rx) = bounded::<()>(1);
            pending.cancel_tx = Some(cancel_tx);
            let _ = evt_tx.send(Evt::FileAccepted { id: id.clone() }).await;
            let path = pending.path.clone();
            let size = pending.size;
            let id_clone = id.clone();
            let evt_tx2 = evt_tx.clone();
            let outbox_tx2 = outbox_tx.clone();
            let transit_key = transfer::derive_transit_key(wh);
            smol::spawn(async move {
                let result = run_send_task(
                    connector,
                    transit_key,
                    their_abilities,
                    their_hints,
                    path,
                    size,
                    id_clone.clone(),
                    evt_tx2.clone(),
                    cancel_rx,
                )
                .await;
                match result {
                    Ok(()) => {
                        // Sender-side success is signaled to UI; the receiver will
                        // confirm with FileDone over mailbox.
                        let _ = evt_tx2
                            .send(Evt::FileProgress {
                                id: id_clone.clone(),
                                bytes: size,
                                total: size,
                                dir: Direction::Out,
                            })
                            .await;
                    }
                    Err(e) => {
                        let _ = outbox_tx2
                            .send(AppMsg::FileCancel {
                                v: PROTOCOL_VERSION,
                                id: id_clone.clone(),
                            })
                            .await;
                        let _ = evt_tx2
                            .send(Evt::FileError {
                                id: id_clone,
                                message: format!("{e}"),
                            })
                            .await;
                    }
                }
            })
            .detach();
        }

        AppMsg::FileReject { id, reason, .. } => {
            outgoing.remove(&id);
            let _ = evt_tx
                .send(Evt::FileError {
                    id,
                    message: format!("对方拒绝: {reason}"),
                })
                .await;
        }

        AppMsg::FileCancel { id, .. } => {
            if let Some(o) = outgoing.get_mut(&id) {
                if let Some(tx) = o.cancel_tx.take() {
                    let _ = tx.send(()).await;
                }
                outgoing.remove(&id);
            }
            if let Some(i) = incoming.get_mut(&id) {
                if let Some(tx) = i.cancel_tx.take() {
                    let _ = tx.send(()).await;
                }
                incoming.remove(&id);
            }
            let _ = evt_tx
                .send(Evt::FileCancelled {
                    id,
                    by: Cancelled::Peer,
                })
                .await;
        }

        AppMsg::FileDone { id, ok, .. } => {
            // Receiver confirmed completion of OUR outgoing transfer.
            outgoing.remove(&id);
            let _ = evt_tx
                .send(Evt::FileDone {
                    id,
                    ok,
                    dir: Direction::Out,
                    save_path: None,
                })
                .await;
        }
    }
    Ok(())
}

// ============================================================
// Send file: build offer, init connector, push offer message
// ============================================================

async fn send_file_offer(
    path: PathBuf,
    wh: &mut Wormhole,
    evt_tx: &Sender<Evt>,
    outgoing: &mut HashMap<String, OutgoingPending>,
    transit_relay: &str,
) -> Result<(), CoreError> {
    tracing::info!("send_file_offer: path={}", path.display());
    let metadata = smol::fs::metadata(&path).await.map_err(|e| {
        tracing::error!("metadata failed for {}: {e:?}", path.display());
        e
    })?;
    if metadata.is_dir() {
        return Err(CoreError::Other(
            "暂不支持发送文件夹，请逐个选择文件".into(),
        ));
    }
    let size = metadata.len();
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .map(String::from)
        .unwrap_or_else(|| "file".into());
    let mime = mime_guess(&name);
    tracing::info!("send_file_offer: name={name} size={size}");

    let connector = transfer::init_connector(transit_relay).await.map_err(|e| {
        tracing::error!("init_connector (send) failed: {e:?}");
        e
    })?;
    let our_hints = transfer::our_hints(&connector);
    let our_abilities = transfer::our_abilities(&connector);
    let id = make_id();

    wh.send_json(&AppMsg::FileOffer {
        v: PROTOCOL_VERSION,
        id: id.clone(),
        name: name.clone(),
        size,
        mime: mime.clone(),
        hints: our_hints,
        abilities: our_abilities,
    })
    .await?;

    outgoing.insert(
        id.clone(),
        OutgoingPending {
            id: id.clone(),
            path,
            name: name.clone(),
            size,
            connector: Some(connector),
            cancel_tx: None,
        },
    );
    let _ = evt_tx.send(Evt::FileOfferSent { id, name, size }).await;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn accept_file(
    id: String,
    save_dir: PathBuf,
    wh: &mut Wormhole,
    evt_tx: &Sender<Evt>,
    outbox_tx: &Sender<AppMsg>,
    incoming: &mut HashMap<String, IncomingPending>,
    transit_relay: &str,
) -> Result<(), CoreError> {
    tracing::info!("accept_file: id={id} save_dir={}", save_dir.display());
    let pending = match incoming.get_mut(&id) {
        Some(p) => p,
        None => {
            tracing::warn!("accept_file: no pending transfer for id={id}");
            return Ok(());
        }
    };
    let connector = transfer::init_connector(transit_relay).await.map_err(|e| {
        tracing::error!("init_connector (recv) failed: {e:?}");
        e
    })?;
    let our_hints = transfer::our_hints(&connector);
    let our_abilities = transfer::our_abilities(&connector);

    wh.send_json(&AppMsg::FileAccept {
        v: PROTOCOL_VERSION,
        id: id.clone(),
        hints: our_hints,
        abilities: our_abilities,
    })
    .await?;

    let save_path = storage::pick_save_path(&pending.name, &save_dir);
    let total = pending.size;
    let their_hints = pending.their_hints.clone();
    let their_abilities = pending.their_abilities;
    let (cancel_tx, cancel_rx) = bounded::<()>(1);
    pending.cancel_tx = Some(cancel_tx);
    let id_clone = id.clone();
    let evt_tx2 = evt_tx.clone();
    let outbox_tx2 = outbox_tx.clone();
    let transit_key = transfer::derive_transit_key(wh);

    smol::spawn(async move {
        let result = run_recv_task(
            connector,
            transit_key,
            their_abilities,
            their_hints,
            save_path.clone(),
            total,
            id_clone.clone(),
            evt_tx2.clone(),
            cancel_rx,
        )
        .await;
        match result {
            Ok(()) => {
                // Notify peer that we wrote the file successfully.
                let _ = outbox_tx2
                    .send(AppMsg::FileDone {
                        v: PROTOCOL_VERSION,
                        id: id_clone.clone(),
                        ok: true,
                    })
                    .await;
                let _ = evt_tx2
                    .send(Evt::FileDone {
                        id: id_clone,
                        ok: true,
                        dir: Direction::In,
                        save_path: Some(save_path.to_string_lossy().to_string()),
                    })
                    .await;
            }
            Err(e) => {
                let _ = outbox_tx2
                    .send(AppMsg::FileCancel {
                        v: PROTOCOL_VERSION,
                        id: id_clone.clone(),
                    })
                    .await;
                let _ = evt_tx2
                    .send(Evt::FileError {
                        id: id_clone,
                        message: format!("{e}"),
                    })
                    .await;
            }
        }
    })
    .detach();

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn run_send_task(
    connector: magic_wormhole::transit::TransitConnector,
    transit_key: magic_wormhole::Key<magic_wormhole::transit::TransitKey>,
    their_abilities: magic_wormhole::transit::Abilities,
    their_hints: magic_wormhole::transit::Hints,
    path: PathBuf,
    size: u64,
    id: String,
    evt_tx: Sender<Evt>,
    cancel_rx: Receiver<()>,
) -> Result<(), CoreError> {
    tracing::info!("run_send_task: id={id} size={size} connecting transit");
    let mut transit = transfer::connect_transit(
        connector,
        TransitRole::Leader,
        transit_key,
        their_abilities,
        their_hints,
    )
    .await
    .map_err(|e| {
        tracing::error!("connect_transit (send) id={id} failed: {e:?}");
        e
    })?;
    tracing::info!("run_send_task: id={id} transit connected, streaming");
    let id_for_progress = id.clone();
    let evt_tx2 = evt_tx.clone();
    transfer::stream_send(
        &mut transit,
        &path,
        size,
        move |bytes| {
            // Best-effort send; if UI closed channel, we ignore.
            let _ = evt_tx2.try_send(Evt::FileProgress {
                id: id_for_progress.clone(),
                bytes,
                total: size,
                dir: Direction::Out,
            });
        },
        cancel_rx,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn run_recv_task(
    connector: magic_wormhole::transit::TransitConnector,
    transit_key: magic_wormhole::Key<magic_wormhole::transit::TransitKey>,
    their_abilities: magic_wormhole::transit::Abilities,
    their_hints: magic_wormhole::transit::Hints,
    save_path: PathBuf,
    size: u64,
    id: String,
    evt_tx: Sender<Evt>,
    cancel_rx: Receiver<()>,
) -> Result<(), CoreError> {
    tracing::info!("run_recv_task: id={id} size={size} connecting transit");
    let mut transit = transfer::connect_transit(
        connector,
        TransitRole::Follower,
        transit_key,
        their_abilities,
        their_hints,
    )
    .await
    .map_err(|e| {
        tracing::error!("connect_transit (recv) id={id} failed: {e:?}");
        e
    })?;
    tracing::info!(
        "run_recv_task: id={id} transit connected, streaming to {}",
        save_path.display()
    );
    let id_for_progress = id.clone();
    let evt_tx2 = evt_tx.clone();
    transfer::stream_recv(
        &mut transit,
        &save_path,
        size,
        move |bytes| {
            let _ = evt_tx2.try_send(Evt::FileProgress {
                id: id_for_progress.clone(),
                bytes,
                total: size,
                dir: Direction::In,
            });
        },
        cancel_rx,
    )
    .await
}

// ============================================================
// Helpers
// ============================================================

/// Generate a numeric short-code password split into two equal halves with a
/// dash, e.g. `123-456`. The full wormhole code becomes `<nameplate>-123-456`.
fn generate_numeric_password(digits: usize) -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let raw: String = (0..digits)
        .map(|_| char::from_digit(rng.gen_range(0..10), 10).unwrap())
        .collect();
    let mid = digits / 2;
    format!("{}-{}", &raw[..mid], &raw[mid..])
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

/// Crude MIME guess based on extension. Phase 3 doesn't need to be precise;
/// just enough to flag executable types in the UI warning path.
fn mime_guess(name: &str) -> Option<String> {
    let ext = name.rsplit('.').next()?.to_ascii_lowercase();
    Some(
        match ext.as_str() {
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "webp" => "image/webp",
            "pdf" => "application/pdf",
            "zip" => "application/zip",
            "txt" | "md" | "log" => "text/plain",
            "json" => "application/json",
            "exe" | "msi" | "bat" | "cmd" | "com" | "scr" | "ps1" => "application/x-msdownload",
            _ => return None,
        }
        .into(),
    )
}

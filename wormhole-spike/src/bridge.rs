//! T1.13 — tokio + smol dual-runtime bridge.
//!
//! Validates the §4 claim: a Tauri-style application (tokio main thread)
//! can host magic-wormhole (smol-only) by spawning a dedicated OS thread
//! that runs `smol::block_on`, with `async-channel` as the IPC.
//!
//! Layout:
//!   tokio main thread
//!     ├── std::thread A → smol::block_on { allocate code, do PAKE, send/recv }
//!     └── std::thread B → smol::block_on { join code, do PAKE, send/recv }
//!
//! Tokio side:
//!   - receives "code" event from A, forwards it as command to B
//!   - awaits "verifier" events from both, asserts they match
//!   - issues "send_text" command to A, "expect_text" to B
//!
//! If everything works, the architecture is settled.

use anyhow::{anyhow, Context, Result};
use async_channel::{bounded, Receiver, Sender};
use serde::{Deserialize, Serialize};
use std::time::Duration;

use magic_wormhole::{transfer, Code, MailboxConnection, Wormhole};

#[derive(Debug, Serialize, Deserialize)]
struct ChatMsg {
    id: String,
    content: String,
}

// ---- Commands sent from tokio side into a session thread ----
#[derive(Debug)]
enum Cmd {
    JoinCode(Code),  // for the "joiner" thread only
    SendText(String),
    ExpectText,      // for the "receiver" thread; reports back whatever it gets
    Close,
}

// ---- Events emitted from a session thread up to tokio ----
#[derive(Debug)]
enum Evt {
    Code(Code),
    PakeDone { verifier_hex: String },
    Sent(String),
    Got(String),
    Closed,
    Error(String),
}

fn smol_session(
    label: &'static str,
    role: Role,
    cmd_rx: Receiver<Cmd>,
    evt_tx: Sender<Evt>,
) {
    smol::block_on(async move {
        let r = run_session(label, role, cmd_rx, &evt_tx).await;
        if let Err(e) = r {
            evt_tx.send(Evt::Error(format!("{label}: {e:#}"))).await.ok();
        } else {
            evt_tx.send(Evt::Closed).await.ok();
        }
    });
}

#[derive(Clone, Copy)]
enum Role { Allocator, Joiner }

async fn run_session(
    label: &'static str,
    role: Role,
    cmd_rx: Receiver<Cmd>,
    evt_tx: &Sender<Evt>,
) -> Result<()> {
    let cfg = transfer::APP_CONFIG.clone();
    let mut wh = match role {
        Role::Allocator => {
            let mc = MailboxConnection::create(cfg, 2).await.context("alloc mailbox")?;
            let code = mc.code().clone();
            evt_tx.send(Evt::Code(code)).await.ok();
            println!("[{label}-smol] code emitted; awaiting PAKE");
            Wormhole::connect(mc).await.context("PAKE")?
        }
        Role::Joiner => {
            let code = match cmd_rx.recv().await? {
                Cmd::JoinCode(c) => c,
                other => return Err(anyhow!("expected JoinCode, got {other:?}")),
            };
            println!("[{label}-smol] joining code {code}");
            let mc = MailboxConnection::connect(cfg, code, true).await.context("claim mailbox")?;
            Wormhole::connect(mc).await.context("PAKE")?
        }
    };

    let v = wh.verifier();
    let verifier_hex = hex::encode(&v.as_slice()[..8]);
    evt_tx.send(Evt::PakeDone { verifier_hex }).await.ok();

    // Process commands (single task event loop; no Mutex on Wormhole, see T1.3)
    use futures::FutureExt;
    use futures::select;
    loop {
        select! {
            cmd = cmd_rx.recv().fuse() => match cmd {
                Ok(Cmd::SendText(s)) => {
                    let m = ChatMsg { id: format!("{label}-{}", uuid_like()), content: s.clone() };
                    wh.send_json(&m).await.context("send_json")?;
                    evt_tx.send(Evt::Sent(s)).await.ok();
                }
                Ok(Cmd::ExpectText) => {
                    let m: ChatMsg = wh.receive_json::<ChatMsg>().await.context("recv outer")??;
                    evt_tx.send(Evt::Got(m.content)).await.ok();
                }
                Ok(Cmd::Close) => {
                    println!("[{label}-smol] closing");
                    return Ok(());
                }
                Ok(other) => return Err(anyhow!("unexpected cmd: {other:?}")),
                Err(_) => return Ok(()),  // tokio side dropped sender
            },
            // Inbound mailbox messages without a pending ExpectText are unexpected here
            // (this minimal bridge only requests via ExpectText).
        }
    }
}

fn uuid_like() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    format!("{n:x}")
}

// hex helper — magic-wormhole ships hex but it's a transient dep; bring our own.
mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        let mut s = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            s.push_str(&format!("{b:02x}"));
        }
        s
    }
}

// ============================================================
// tokio side
// ============================================================

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<()> {
    println!("=== T1.13  tokio (main) + smol (session threads) bridge ===");
    println!("Tokio is in charge; magic-wormhole runs on dedicated smol threads.\n");

    // Two sessions: A (allocator) and B (joiner)
    let (a_cmd_tx, a_cmd_rx) = bounded::<Cmd>(8);
    let (a_evt_tx, a_evt_rx) = bounded::<Evt>(16);
    let (b_cmd_tx, b_cmd_rx) = bounded::<Cmd>(8);
    let (b_evt_tx, b_evt_rx) = bounded::<Evt>(16);

    let _a_thread = std::thread::Builder::new().name("smol-A".into()).spawn(move || {
        smol_session("A", Role::Allocator, a_cmd_rx, a_evt_tx);
    })?;
    let _b_thread = std::thread::Builder::new().name("smol-B".into()).spawn(move || {
        smol_session("B", Role::Joiner, b_cmd_rx, b_evt_tx);
    })?;

    // Step 1: receive code from A (smol→tokio crossing)
    let code = match a_evt_rx.recv().await? {
        Evt::Code(c) => c,
        other => return Err(anyhow!("expected Code, got {other:?}")),
    };
    println!("[tokio] got code from A: {code}");

    // Step 2: forward code to B (tokio→smol crossing)
    b_cmd_tx.send(Cmd::JoinCode(code)).await?;
    println!("[tokio] forwarded code to B; awaiting PAKE on both");

    // Step 3: await PakeDone from both, assert verifier match
    let va = match a_evt_rx.recv().await? {
        Evt::PakeDone { verifier_hex } => verifier_hex,
        other => return Err(anyhow!("A: expected PakeDone, got {other:?}")),
    };
    let vb = match b_evt_rx.recv().await? {
        Evt::PakeDone { verifier_hex } => verifier_hex,
        other => return Err(anyhow!("B: expected PakeDone, got {other:?}")),
    };
    if va != vb {
        return Err(anyhow!("verifier mismatch: A={va} B={vb}"));
    }
    println!("[tokio] PAKE done on both threads, verifier prefix = {va} ✓");

    // Step 4: tokio sends a chat command to A; B should receive it
    println!("\n[tokio] orchestrating round-trip messages over the bridge");
    b_cmd_tx.send(Cmd::ExpectText).await?;
    a_cmd_tx.send(Cmd::SendText("hello from tokio-orchestrated wormhole".into())).await?;
    let _ = match a_evt_rx.recv().await? {
        Evt::Sent(s) => println!("[tokio] A reported sent: {s:?}"),
        other => return Err(anyhow!("A: expected Sent, got {other:?}")),
    };
    let got = match b_evt_rx.recv().await? {
        Evt::Got(s) => s,
        other => return Err(anyhow!("B: expected Got, got {other:?}")),
    };
    println!("[tokio] B reported got: {got:?}");

    // Step 5: reverse direction
    a_cmd_tx.send(Cmd::ExpectText).await?;
    b_cmd_tx.send(Cmd::SendText("reply on the same wormhole".into())).await?;
    match b_evt_rx.recv().await? {
        Evt::Sent(s) => println!("[tokio] B reported sent: {s:?}"),
        other => return Err(anyhow!("B: expected Sent, got {other:?}")),
    }
    let got2 = match a_evt_rx.recv().await? {
        Evt::Got(s) => s,
        other => return Err(anyhow!("A: expected Got, got {other:?}")),
    };
    println!("[tokio] A reported got: {got2:?}");

    // Step 6: prove tokio's own runtime is alive — schedule a tokio::time::sleep
    println!("\n[tokio] sanity: tokio runtime still responsive");
    tokio::time::sleep(Duration::from_millis(300)).await;
    println!("[tokio] tokio sleep ok");

    // Step 7: shut down
    a_cmd_tx.send(Cmd::Close).await?;
    b_cmd_tx.send(Cmd::Close).await?;
    drop(a_cmd_tx);
    drop(b_cmd_tx);

    // Wait for clean closure events
    while let Ok(e) = a_evt_rx.recv().await { if matches!(e, Evt::Closed) { break; } }
    while let Ok(e) = b_evt_rx.recv().await { if matches!(e, Evt::Closed) { break; } }

    println!("\nT1.13 PASS: tokio↔smol bridge end-to-end works");
    Ok(())
}

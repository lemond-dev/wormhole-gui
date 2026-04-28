//! Tier 1 spike for the wormhole-gui v0.1 architecture.
//!
//! Two peers run in-process as smol tasks talking to the public mailbox relay.
//!
//!   T1.1  Wormhole stays open after PAKE; the mailbox carries many
//!         bidirectional JSON messages over an extended period.
//!   T1.2  An extended round-trip burst (50 msgs) including
//!         large payloads, simulating multiple file-offer dances on the
//!         same wormhole. Pure "is the mailbox really persistent?" stress.
//!         Note: actually wiring up two real transit connections requires
//!         considerably more code; this is intentionally scoped down — the
//!         architectural feasibility is established by reading the source
//!         (transit::init / TransitConnector are independent of the
//!         Wormhole; transfer::send_file is the API that consumes Wormhole
//!         and is therefore unusable for session-style v0.1).
//!   T1.3  Concurrent send + receive on a single Wormhole via Arc<Mutex>.
//!
//! Run:  cargo run --release -- all
//!       cargo run --release -- t1 | t2 | t3

use anyhow::{anyhow, Context, Result};
use async_channel::{bounded, Sender};
use async_lock::Mutex;
use futures::{future::try_join, FutureExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};

use magic_wormhole::{
    transfer,
    transit::{self, Abilities, DirectHint, Hints, RelayHint, TransitKey, TransitRole, DEFAULT_RELAY_SERVER},
    AppConfig, AppID, Code, MailboxConnection, Wormhole,
};

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
enum AppMsg {
    #[serde(rename = "text")]
    Text { id: String, content: String, ts: u64 },
    #[serde(rename = "ping")]
    Ping { seq: u32 },
    #[serde(rename = "pong")]
    Pong { seq: u32 },
    #[serde(rename = "blob")]
    Blob { id: String, payload: String }, // big payload as base64-ish text
    #[serde(rename = "bye")]
    Bye,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

fn app_config() -> AppConfig<transfer::AppVersion> {
    transfer::APP_CONFIG.clone()
}

/// Allocate a mailbox and return (mc, code). Caller MUST publish the code
/// to the joiner BEFORE awaiting Wormhole::connect, since PAKE only finishes
/// after the joiner shows up.
async fn allocate_mailbox() -> Result<(MailboxConnection<transfer::AppVersion>, Code)> {
    let mc = MailboxConnection::create(app_config(), 2)
        .await
        .context("alloc mailbox")?;
    let code = mc.code().clone();
    Ok((mc, code))
}

async fn join_with_code(code: Code) -> Result<Wormhole> {
    let mc = MailboxConnection::connect(app_config(), code, true)
        .await
        .context("claim mailbox")?;
    Wormhole::connect(mc).await.context("PAKE (joiner)")
}

async fn recv_msg(wh: &mut Wormhole) -> Result<AppMsg> {
    let outer = wh.receive_json::<AppMsg>().await.context("wh.receive")?;
    outer.context("json decode")
}

// ---------------------------------------------------------------------------
// T1.1 — persistent mailbox, repeated bidirectional JSON
// ---------------------------------------------------------------------------
async fn t1_persistent_mailbox() -> Result<()> {
    println!("\n=== T1.1  persistent mailbox / 5 round trips ===");
    let (code_tx, code_rx) = bounded::<Code>(1);

    let a = async move {
        let (mc, code) = allocate_mailbox().await?;
        println!("A: code = {}", code);
        code_tx.send(code).await.ok();
        let mut wh = Wormhole::connect(mc).await.context("A: PAKE")?;
        let v = wh.verifier();
        println!("A: verifier first 4 bytes = {:02x?}", &v.as_slice()[..4]);

        for seq in 0..5u32 {
            wh.send_json(&AppMsg::Ping { seq }).await?;
            println!("A → ping {}", seq);
            let m = recv_msg(&mut wh).await?;
            println!("A ← {:?}", m);
            smol::Timer::after(Duration::from_millis(400)).await;
        }
        wh.send_json(&AppMsg::Bye).await.ok();
        anyhow::Ok(())
    };

    let b = async move {
        let code = code_rx.recv().await?;
        let mut wh = join_with_code(code).await?;
        let v = wh.verifier();
        println!("B: verifier first 4 bytes = {:02x?}", &v.as_slice()[..4]);

        for seq in 0..5u32 {
            let m = recv_msg(&mut wh).await?;
            println!("B ← {:?}", m);
            wh.send_json(&AppMsg::Pong { seq }).await?;
            wh.send_json(&AppMsg::Text {
                id: format!("t-{seq}"),
                content: format!("hello from B #{seq}"),
                ts: now_ms(),
            })
            .await?;
        }
        anyhow::Ok(())
    };

    let started = Instant::now();
    try_join(a, b).await?;
    println!(
        "T1.1 PASS: 5 round trips in {:?}, mailbox stayed open.",
        started.elapsed()
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// T1.2 — extended burst with large payloads, simulating multiple file
//        offer/accept rounds on the same wormhole.
// ---------------------------------------------------------------------------
async fn t1_extended_burst() -> Result<()> {
    println!("\n=== T1.2  extended burst + large payloads ===");
    let (code_tx, code_rx) = bounded::<Code>(1);
    const ROUNDS: u32 = 25;
    const BLOB_BYTES: usize = 4 * 1024;

    let a = async move {
        let (mc, code) = allocate_mailbox().await?;
        code_tx.send(code).await.ok();
        let mut wh = Wormhole::connect(mc).await.context("A: PAKE")?;
        let blob = "x".repeat(BLOB_BYTES);
        for i in 0..ROUNDS {
            // simulate a "file offer" (large-ish JSON over mailbox)
            wh.send_json(&AppMsg::Blob {
                id: format!("offer-{i}"),
                payload: blob.clone(),
            })
            .await?;
            // simulate "ack" coming back
            let m = recv_msg(&mut wh).await?;
            if !matches!(m, AppMsg::Pong { .. }) {
                return Err(anyhow!("A: unexpected ack: {m:?}"));
            }
            if i % 5 == 0 {
                println!("A: round {i} acked");
            }
        }
        wh.send_json(&AppMsg::Bye).await.ok();
        anyhow::Ok(())
    };

    let b = async move {
        let code = code_rx.recv().await?;
        let mut wh = join_with_code(code).await?;
        for i in 0..ROUNDS {
            let m = recv_msg(&mut wh).await?;
            match m {
                AppMsg::Blob { id, payload } => {
                    if payload.len() != BLOB_BYTES {
                        return Err(anyhow!("B: blob size {} != {BLOB_BYTES}", payload.len()));
                    }
                    let _ = id;
                },
                other => return Err(anyhow!("B: unexpected msg #{i}: {other:?}")),
            }
            wh.send_json(&AppMsg::Pong { seq: i }).await?;
        }
        anyhow::Ok(())
    };

    let started = Instant::now();
    try_join(a, b).await?;
    println!(
        "T1.2 PASS: {ROUNDS} rounds × {BLOB_BYTES}-byte blobs in {:?}.",
        started.elapsed()
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// T1.3 — concurrent send + receive on a single Wormhole
// ---------------------------------------------------------------------------
async fn t1_concurrent_io() -> Result<()> {
    println!("\n=== T1.3  concurrent send + receive (single event loop + select) ===");
    // FINDING from earlier attempt: Arc<Mutex<Wormhole>> deadlocks. receive_json
    // holds the lock across an indefinite await waiting for the next peer
    // message, starving any send. The Wormhole API in 0.8 does not provide a
    // split() into independent send/recv halves.
    //
    // Correct pattern: single task owns the Wormhole and does futures::select!
    // between (a) outbound message channel and (b) wh.receive(). Application
    // code on either side talks to the loop via channels.
    let (code_tx, code_rx) = bounded::<Code>(1);

    async fn run_peer(mut wh: Wormhole, label: &'static str) -> Result<()> {
        use futures::{select, FutureExt};
        let (out_tx, out_rx) = bounded::<AppMsg>(8);

        // Driver task that injects 5 outbound messages from the "UI side".
        let driver = {
            let out_tx = out_tx.clone();
            let label = label;
            smol::spawn(async move {
                for i in 0..5 {
                    out_tx
                        .send(AppMsg::Text {
                            id: format!("{label}-{i}"),
                            content: format!("{label} msg {i}"),
                            ts: now_ms(),
                        })
                        .await?;
                    smol::Timer::after(Duration::from_millis(200)).await;
                }
                drop(out_tx);
                anyhow::Ok(())
            })
        };
        drop(out_tx); // driver clone keeps it alive; loop sees close after driver drops it

        let mut got_in = 0;
        let mut sent_out = 0;
        loop {
            select! {
                outbound = out_rx.recv().fuse() => match outbound {
                    Ok(m) => {
                        wh.send_json(&m).await?;
                        sent_out += 1;
                        println!("{label} → {m:?}");
                    },
                    Err(_) => {
                        // sender side closed; keep draining inbound until target reached
                    }
                },
                inbound = wh.receive_json::<AppMsg>().fuse() => {
                    let m = inbound??;
                    got_in += 1;
                    println!("{label} ← {m:?}");
                }
            }
            if sent_out >= 5 && got_in >= 5 {
                break;
            }
        }
        driver.await?;
        Ok(())
    }

    let a = async move {
        let (mc, code) = allocate_mailbox().await?;
        code_tx.send(code).await.ok();
        let wh = Wormhole::connect(mc).await.context("A: PAKE")?;
        run_peer(wh, "A").await
    };
    let b = async move {
        let code = code_rx.recv().await?;
        let wh = join_with_code(code).await?;
        run_peer(wh, "B").await
    };

    let started = Instant::now();
    try_join(a, b).await?;
    println!(
        "T1.3 PASS: 5+5 concurrent messages in {:?} (no deadlock).",
        started.elapsed()
    );
    Ok(())
}

// ---------------------------------------------------------------------------
// T1.4 — REAL transit transfer at multiple sizes, same Wormhole reused
// ---------------------------------------------------------------------------
#[derive(Debug, Serialize, Deserialize)]
struct TransitHandshake {
    abilities: Abilities,
    hints: Hints,
}

async fn run_transit_round(
    wh: &mut Wormhole,
    role: TransitRole,
    label: &'static str,
    size: u64,
) -> Result<(Duration, Duration, u64)> {
    use magic_wormhole::KeyPurpose;
    let _ = std::marker::PhantomData::<TransitKey>;

    let abilities = Abilities::ALL;
    let relay_hint = RelayHint::new(
        None,
        [DirectHint::new("transit.magic-wormhole.io", 4001)],
        [],
    );

    let setup_started = Instant::now();
    let connector = transit::init(abilities, None, vec![relay_hint])
        .await
        .context("transit::init")?;
    let our_msg = TransitHandshake {
        abilities: *connector.our_abilities(),
        hints: (**connector.our_hints()).clone(),
    };
    wh.send_json(&our_msg).await.context("send transit hs")?;
    let their: TransitHandshake = wh
        .receive_json::<TransitHandshake>()
        .await
        .context("recv transit hs (outer)")??;

    let appid_str = wh.appid().as_ref().to_string();
    let purpose = format!("{appid_str}/transit-key");
    let transit_key = wh
        .key()
        .derive_subkey_from_purpose::<TransitKey>(&purpose);

    let (mut transit, info) = connector
        .connect(role.clone(), transit_key, their.abilities, std::sync::Arc::new(their.hints))
        .await
        .context("connector.connect")?;
    let setup = setup_started.elapsed();
    println!("{label}: transit established ({:?}) via {info:?}", setup);

    let xfer_started = Instant::now();
    let bytes;
    match role {
        TransitRole::Leader => {
            let chunk = vec![0xabu8; 16 * 1024];
            let mut remaining = size;
            while remaining > 0 {
                let n = std::cmp::min(remaining, chunk.len() as u64) as usize;
                transit.send_record(&chunk[..n]).await.context("send_record")?;
                remaining -= n as u64;
            }
            transit.flush().await.context("transit flush")?;
            bytes = size;
        },
        TransitRole::Follower => {
            let mut received = 0u64;
            while received < size {
                let buf = transit.receive_record().await.context("receive_record")?;
                received += buf.len() as u64;
            }
            bytes = received;
        },
    }
    let xfer = xfer_started.elapsed();
    Ok((setup, xfer, bytes))
}

async fn t1_real_transit() -> Result<()> {
    println!("\n=== T1.4  real transit, multiple sizes on one Wormhole ===");
    let sizes: Vec<(u64, &'static str)> = vec![
        (1_024, "1 KB"),
        (1 << 20, "1 MB"),
        (100 << 20, "100 MB"),
    ];
    let (code_tx, code_rx) = bounded::<Code>(1);

    let a = async move {
        let (mc, code) = allocate_mailbox().await?;
        code_tx.send(code).await.ok();
        let mut wh = Wormhole::connect(mc).await.context("A: PAKE")?;
        for (sz, name) in &sizes {
            println!("--- A round: {name} ({sz} bytes, leader) ---");
            let (setup, xfer, bytes) =
                run_transit_round(&mut wh, TransitRole::Leader, "A", *sz).await?;
            let mbps = (bytes as f64 / xfer.as_secs_f64()) / 1_000_000.0;
            println!(
                "A: {name} sent {bytes} bytes in {xfer:?} ({mbps:.2} MB/s, setup {setup:?})"
            );
        }
        anyhow::Ok(())
    };

    let b = async move {
        let code = code_rx.recv().await?;
        let mut wh = join_with_code(code).await?;
        // B uses the same `sizes` list — re-derive locally.
        let sizes: Vec<(u64, &'static str)> = vec![
            (1_024, "1 KB"),
            (1 << 20, "1 MB"),
            (100 << 20, "100 MB"),
        ];
        for (sz, name) in &sizes {
            println!("--- B round: {name} ({sz} bytes, follower) ---");
            let (setup, xfer, bytes) =
                run_transit_round(&mut wh, TransitRole::Follower, "B", *sz).await?;
            let mbps = (bytes as f64 / xfer.as_secs_f64()) / 1_000_000.0;
            println!(
                "B: {name} got {bytes} bytes in {xfer:?} ({mbps:.2} MB/s, setup {setup:?})"
            );
            if bytes != *sz {
                return Err(anyhow!("size mismatch: got {bytes} expected {sz}"));
            }
        }
        anyhow::Ok(())
    };

    let started = Instant::now();
    try_join(a, b).await?;
    println!("T1.4 PASS: 3 transit rounds on one wormhole, total {:?}", started.elapsed());
    Ok(())
}

// ---------------------------------------------------------------------------
// T1.5 — mailbox single-message size ceiling (exponential doubling)
// ---------------------------------------------------------------------------
async fn t1_max_msg_size() -> Result<()> {
    println!("\n=== T1.5  mailbox max single-message size ===");
    // Sizes are payload length (bytes of 'x' characters wrapped in a
    // {"payload":"xxx..."} JSON envelope; wire bytes are slightly larger).
    let sizes: &[usize] = &[
        1 << 10,   // 1 KB
        4 << 10,   // 4 KB
        16 << 10,  // 16 KB
        64 << 10,  // 64 KB
        256 << 10, // 256 KB
        1 << 20,   // 1 MB
        4 << 20,   // 4 MB
        16 << 20,  // 16 MB
    ];
    let timeout = Duration::from_secs(45);

    let (code_tx, code_rx) = bounded::<Code>(1);
    let (result_tx, result_rx) = bounded::<(usize, Result<Duration, String>)>(64);

    #[derive(Serialize, Deserialize)]
    struct Probe { size: usize, payload: String }

    let a = async move {
        let (mc, code) = allocate_mailbox().await?;
        code_tx.send(code).await.ok();
        let mut wh = Wormhole::connect(mc).await.context("A: PAKE")?;
        for &sz in sizes {
            let probe = Probe { size: sz, payload: "x".repeat(sz) };
            let started = Instant::now();
            let r = smol::future::or(
                async {
                    wh.send_json(&probe).await.map_err(|e| format!("send: {e}"))?;
                    let echoed: Probe = wh
                        .receive_json::<Probe>()
                        .await
                        .map_err(|e| format!("recv outer: {e}"))?
                        .map_err(|e| format!("recv json: {e}"))?;
                    if echoed.size != sz || echoed.payload.len() != sz {
                        return Err(format!("size mismatch: got {} (payload {})", echoed.size, echoed.payload.len()));
                    }
                    Ok(started.elapsed())
                },
                async {
                    smol::Timer::after(timeout).await;
                    Err::<Duration, String>(format!("timeout after {timeout:?}"))
                },
            ).await;
            let failed = r.is_err();
            result_tx.send((sz, r)).await.ok();
            if failed { break; }
        }
        drop(result_tx);
        anyhow::Ok(())
    };

    let b = async move {
        let code = code_rx.recv().await?;
        let mut wh = join_with_code(code).await?;
        loop {
            let echo_or_err = smol::future::or(
                async {
                    let p: Probe = wh.receive_json::<Probe>().await??;
                    Ok::<_, anyhow::Error>(Some(p))
                },
                async {
                    smol::Timer::after(timeout).await;
                    anyhow::Ok::<Option<Probe>>(None)
                },
            ).await;
            match echo_or_err {
                Ok(Some(p)) => {
                    if let Err(e) = wh.send_json(&p).await {
                        eprintln!("B: echo send failed: {e}");
                        break;
                    }
                },
                Ok(None) => break,    // timeout, A likely gave up
                Err(_) => break,      // recv error, wormhole probably dead
            }
        }
        anyhow::Ok(())
    };

    let consumer = async move {
        let mut last_ok: Option<usize> = None;
        let mut first_fail: Option<(usize, String)> = None;
        while let Ok((sz, res)) = result_rx.recv().await {
            match res {
                Ok(d) => {
                    let mb = sz as f64 / (1024.0 * 1024.0);
                    let mbps = (sz as f64 * 2.0) / d.as_secs_f64() / 1_000_000.0;
                    println!("✅ {} ({:.3} MB) round-trip {:?} ~{mbps:.1} MB/s", human(sz), mb, d);
                    last_ok = Some(sz);
                },
                Err(e) => {
                    println!("❌ {} FAILED: {}", human(sz), e);
                    first_fail = Some((sz, e));
                    break;
                },
            }
        }
        anyhow::Ok((last_ok, first_fail))
    };

    let started = Instant::now();
    let ((), (), (last_ok, first_fail)) =
        futures::future::try_join3(a, b, consumer).await?;
    println!("T1.5 finished in {:?}", started.elapsed());
    match (last_ok, first_fail) {
        (Some(ok), Some((fail, _))) =>
            println!("  -> last OK: {}, first FAIL: {}  (limit between them)", human(ok), human(fail)),
        (Some(ok), None) =>
            println!("  -> all sizes up to {} succeeded", human(ok)),
        (None, Some((fail, e))) =>
            println!("  -> first attempt FAILED at {}: {}", human(fail), e),
        (None, None) =>
            println!("  -> no data"),
    }
    Ok(())
}

fn human(n: usize) -> String {
    if n >= 1 << 20 { format!("{} MB", n >> 20) }
    else if n >= 1 << 10 { format!("{} KB", n >> 10) }
    else { format!("{} B", n) }
}

// ===========================================================================
//  Tier 1.5 — additional unverified items requested 2026-04-28
// ===========================================================================
//   T1.6   transit dies / cancelled mid-transfer → wormhole still usable?  (#1, #4)
//   T1.7   reverse-direction concurrent transit on one wormhole              (#2)
//   T1.8   mailbox idle survival (5 minutes silent)                          (#3)
//   T1.9   PAKE failure with wrong password                                  (#5)
//   T1.10  long-run stability: 10 × 1MB transit over ~2 minutes              (#6)
//   T1.11  memory stability for 5MB / 50MB / 500MB                            (#7)
//   T1.12  Unicode text + Chinese filename                                    (#8)

fn rss_mb() -> f64 {
    memory_stats::memory_stats()
        .map(|s| s.physical_mem as f64 / 1_048_576.0)
        .unwrap_or(0.0)
}

// ---------------- T1.6 transit drop, wormhole survives ------------------
async fn t1_transit_drop() -> Result<()> {
    println!("\n=== T1.6  transit dropped mid-transfer; wormhole should survive ===");
    let (code_tx, code_rx) = bounded::<Code>(1);
    const SIZE: u64 = 32 << 20; // 32MB

    let a = async move {
        let (mc, code) = allocate_mailbox().await?;
        code_tx.send(code).await.ok();
        let mut wh = Wormhole::connect(mc).await.context("A: PAKE")?;

        // Round 1: start transit, receiver will abort halfway
        let abilities = Abilities::ALL;
        let connector = transit::init(abilities, None, vec![]).await?;
        let our = TransitHandshake { abilities: *connector.our_abilities(), hints: (**connector.our_hints()).clone() };
        wh.send_json(&our).await?;
        let their: TransitHandshake = wh.receive_json::<TransitHandshake>().await??;
        let purpose = format!("{}/transit-key", wh.appid().as_ref());
        let key = wh.key().derive_subkey_from_purpose::<TransitKey>(&purpose);
        let (mut transit, _) = connector.connect(TransitRole::Leader, key, their.abilities, std::sync::Arc::new(their.hints)).await?;
        let chunk = vec![0u8; 16 * 1024];
        let mut sent = 0u64;
        let result = loop {
            if sent >= SIZE { break Ok(()); }
            let n = std::cmp::min(SIZE - sent, chunk.len() as u64) as usize;
            match transit.send_record(&chunk[..n]).await {
                Ok(_) => { sent += n as u64; }
                Err(e) => break Err(e),
            }
        };
        drop(transit);
        println!("A: send loop ended after {sent} bytes, result = {:?}", result.as_ref().err().map(|e| e.to_string()));

        // Round 2: try to use wormhole again
        wh.send_json(&AppMsg::Text { id: "post-abort".into(), content: "still alive?".into(), ts: now_ms() }).await
            .context("A: post-abort send")?;
        let echoed: AppMsg = wh.receive_json::<AppMsg>().await
            .context("A: post-abort recv outer")?
            .context("A: post-abort recv json")?;
        println!("A: post-abort recv = {:?}", echoed);
        anyhow::Ok(())
    };

    let b = async move {
        let code = code_rx.recv().await?;
        let mut wh = join_with_code(code).await?;
        let abilities = Abilities::ALL;
        let connector = transit::init(abilities, None, vec![]).await?;
        let our = TransitHandshake { abilities: *connector.our_abilities(), hints: (**connector.our_hints()).clone() };
        wh.send_json(&our).await?;
        let their: TransitHandshake = wh.receive_json::<TransitHandshake>().await??;
        let purpose = format!("{}/transit-key", wh.appid().as_ref());
        let key = wh.key().derive_subkey_from_purpose::<TransitKey>(&purpose);
        let (mut transit, _) = connector.connect(TransitRole::Follower, key, their.abilities, std::sync::Arc::new(their.hints)).await?;
        let mut got = 0u64;
        let target = SIZE / 2;
        while got < target {
            match transit.receive_record().await {
                Ok(buf) => got += buf.len() as u64,
                Err(_) => break,
            }
        }
        println!("B: aborting receive at {got} bytes (target was {target})");
        drop(transit);

        // Echo back on wormhole if A sends
        if let Ok(Ok(m)) = wh.receive_json::<AppMsg>().await {
            wh.send_json(&m).await.ok();
            println!("B: post-abort echoed: {:?}", m);
        }
        anyhow::Ok(())
    };

    try_join(a, b).await?;
    println!("T1.6 PASS: wormhole still usable after transit was killed");
    Ok(())
}

// ---------------- T1.7 reverse-direction concurrent transit ------------------
async fn t1_reverse_concurrent() -> Result<()> {
    println!("\n=== T1.7  reverse-direction concurrent transit (A→B and B→A simultaneously) ===");
    const SIZE: u64 = 8 << 20; // 8MB each

    let (code_tx, code_rx) = bounded::<Code>(1);

    async fn one_side(mut wh: Wormhole, label: &'static str) -> Result<()> {
        // Each side: handshake-as-sender, then handshake-as-receiver, then run both transits in parallel.
        // We use deterministic ordering: A→B handshake first, then B→A handshake.
        let abilities = Abilities::ALL;

        // ---- A→B (A is leader, B is follower) ----
        let conn_ab = transit::init(abilities, None, vec![]).await?;
        let me_ab = TransitHandshake { abilities: *conn_ab.our_abilities(), hints: (**conn_ab.our_hints()).clone() };
        wh.send_json(&me_ab).await?;
        let peer_ab: TransitHandshake = wh.receive_json::<TransitHandshake>().await??;

        // ---- B→A (B is leader, A is follower) ----
        let conn_ba = transit::init(abilities, None, vec![]).await?;
        let me_ba = TransitHandshake { abilities: *conn_ba.our_abilities(), hints: (**conn_ba.our_hints()).clone() };
        wh.send_json(&me_ba).await?;
        let peer_ba: TransitHandshake = wh.receive_json::<TransitHandshake>().await??;

        let purpose = format!("{}/transit-key", wh.appid().as_ref());
        let key_ab = wh.key().derive_subkey_from_purpose::<TransitKey>(&purpose);
        let key_ba = wh.key().derive_subkey_from_purpose::<TransitKey>(&purpose);

        // Roles: A is Leader for A→B, Follower for B→A
        let (role_ab, role_ba) = if label == "A" {
            (TransitRole::Leader, TransitRole::Follower)
        } else {
            (TransitRole::Follower, TransitRole::Leader)
        };

        let started = Instant::now();
        let task_ab = async {
            let (mut t, _) = conn_ab.connect(role_ab.clone(), key_ab, peer_ab.abilities, std::sync::Arc::new(peer_ab.hints)).await?;
            match role_ab {
                TransitRole::Leader => {
                    let chunk = vec![0xaau8; 16*1024];
                    let mut rem = SIZE;
                    while rem > 0 {
                        let n = std::cmp::min(rem, chunk.len() as u64) as usize;
                        t.send_record(&chunk[..n]).await?;
                        rem -= n as u64;
                    }
                    t.flush().await?;
                }
                TransitRole::Follower => {
                    let mut got = 0;
                    while got < SIZE {
                        let buf = t.receive_record().await?;
                        got += buf.len() as u64;
                    }
                }
            }
            anyhow::Ok(())
        };
        let task_ba = async {
            let (mut t, _) = conn_ba.connect(role_ba.clone(), key_ba, peer_ba.abilities, std::sync::Arc::new(peer_ba.hints)).await?;
            match role_ba {
                TransitRole::Leader => {
                    let chunk = vec![0xbbu8; 16*1024];
                    let mut rem = SIZE;
                    while rem > 0 {
                        let n = std::cmp::min(rem, chunk.len() as u64) as usize;
                        t.send_record(&chunk[..n]).await?;
                        rem -= n as u64;
                    }
                    t.flush().await?;
                }
                TransitRole::Follower => {
                    let mut got = 0;
                    while got < SIZE {
                        let buf = t.receive_record().await?;
                        got += buf.len() as u64;
                    }
                }
            }
            anyhow::Ok(())
        };
        try_join(task_ab, task_ba).await?;
        println!("{label}: both transits finished in {:?}", started.elapsed());
        Ok(())
    }

    let a = async move {
        let (mc, code) = allocate_mailbox().await?;
        code_tx.send(code).await.ok();
        let wh = Wormhole::connect(mc).await.context("A: PAKE")?;
        one_side(wh, "A").await
    };
    let b = async move {
        let code = code_rx.recv().await?;
        let wh = join_with_code(code).await?;
        one_side(wh, "B").await
    };
    let started = Instant::now();
    try_join(a, b).await?;
    println!("T1.7 PASS: 8MB each direction in parallel, total {:?}", started.elapsed());
    Ok(())
}

// ---------------- T1.8 mailbox idle survival ------------------
async fn t1_mailbox_idle(idle_secs: u64) -> Result<()> {
    println!("\n=== T1.8  mailbox idle for {idle_secs}s, then exchange ===");
    let (code_tx, code_rx) = bounded::<Code>(1);

    let a = async move {
        let (mc, code) = allocate_mailbox().await?;
        code_tx.send(code).await.ok();
        let mut wh = Wormhole::connect(mc).await.context("A: PAKE")?;
        println!("A: PAKE ok at t=0; sleeping {idle_secs}s");
        smol::Timer::after(Duration::from_secs(idle_secs)).await;
        let started = Instant::now();
        wh.send_json(&AppMsg::Ping { seq: 0 }).await.context("A: send after idle")?;
        let m = wh.receive_json::<AppMsg>().await.context("A: recv outer")??;
        println!("A: post-idle round-trip {:?}, got {:?}", started.elapsed(), m);
        anyhow::Ok(())
    };
    let b = async move {
        let code = code_rx.recv().await?;
        let mut wh = join_with_code(code).await?;
        smol::Timer::after(Duration::from_secs(idle_secs)).await;
        let m: AppMsg = wh.receive_json::<AppMsg>().await.context("B: recv outer")??;
        println!("B: got {m:?} after idle");
        wh.send_json(&AppMsg::Pong { seq: 0 }).await?;
        anyhow::Ok(())
    };
    try_join(a, b).await?;
    println!("T1.8 PASS: mailbox survived {idle_secs}s of silence");
    Ok(())
}

// ---------------- T1.9 PAKE failure ------------------
async fn t1_pake_fail() -> Result<()> {
    println!("\n=== T1.9  PAKE failure (wrong password) ===");
    let (code_tx, code_rx) = bounded::<Code>(1);

    let a = async move {
        let (mc, code) = allocate_mailbox().await?;
        code_tx.send(code).await.ok();
        let r = Wormhole::connect(mc).await;
        match r {
            Ok(_) => Err(anyhow!("A: expected error but PAKE succeeded")),
            Err(e) => {
                println!("A: PAKE error (expected): {e:?}");
                anyhow::Ok(())
            }
        }
    };
    let b = async move {
        let good_code = code_rx.recv().await?;
        // Twist the password: keep nameplate, replace password with wrong word
        use std::str::FromStr;
        let raw = good_code.to_string();
        let nameplate = raw.split('-').next().unwrap();
        let wrong = format!("{nameplate}-wrong-secret-words-aaaa");
        let bad_code = Code::from_str(&wrong)?;
        println!("B: trying bad code {bad_code}");
        let mc = MailboxConnection::connect(app_config(), bad_code, true).await?;
        let r = Wormhole::connect(mc).await;
        match r {
            Ok(_) => Err(anyhow!("B: expected error but PAKE succeeded")),
            Err(e) => {
                println!("B: PAKE error (expected): {e:?}");
                anyhow::Ok(())
            }
        }
    };
    try_join(a, b).await?;
    println!("T1.9 PASS: PAKE failure observed cleanly on both sides");
    Ok(())
}

// ---------------- T1.10 long-run stability ------------------
async fn t1_long_run(rounds: u32, gap_secs: u64) -> Result<()> {
    println!("\n=== T1.10  long-run: {rounds} × 1MB transit with {gap_secs}s gap ===");
    let (code_tx, code_rx) = bounded::<Code>(1);
    const SIZE: u64 = 1 << 20;

    let a = async move {
        let (mc, code) = allocate_mailbox().await?;
        code_tx.send(code).await.ok();
        let mut wh = Wormhole::connect(mc).await.context("A: PAKE")?;
        for i in 0..rounds {
            let (_setup, xfer, _) = run_transit_round(&mut wh, TransitRole::Leader, "A", SIZE).await?;
            let mb = SIZE as f64 / xfer.as_secs_f64() / 1_000_000.0;
            println!("A: round {i}/{rounds} sent in {xfer:?} ({mb:.1} MB/s) RSS={:.1}MB", rss_mb());
            smol::Timer::after(Duration::from_secs(gap_secs)).await;
        }
        anyhow::Ok(())
    };
    let b = async move {
        let code = code_rx.recv().await?;
        let mut wh = join_with_code(code).await?;
        for i in 0..rounds {
            let (_setup, xfer, bytes) = run_transit_round(&mut wh, TransitRole::Follower, "B", SIZE).await?;
            if bytes != SIZE { return Err(anyhow!("size mismatch at round {i}")); }
            let _ = xfer;
            smol::Timer::after(Duration::from_secs(gap_secs)).await;
        }
        anyhow::Ok(())
    };
    let started = Instant::now();
    try_join(a, b).await?;
    println!("T1.10 PASS: {rounds} rounds in {:?}", started.elapsed());
    Ok(())
}

// ---------------- T1.11 memory stability ------------------
async fn t1_memory() -> Result<()> {
    println!("\n=== T1.11  memory stability across sizes ===");
    let sizes: Vec<(u64, &str)> = vec![
        (5 << 20, "5MB"),
        (50 << 20, "50MB"),
        (500 << 20, "500MB"),
    ];
    let (code_tx, code_rx) = bounded::<Code>(1);

    let a = async move {
        let (mc, code) = allocate_mailbox().await?;
        code_tx.send(code).await.ok();
        let mut wh = Wormhole::connect(mc).await.context("A: PAKE")?;
        for (sz, name) in &sizes {
            let before = rss_mb();
            let (_, xfer, _) = run_transit_round(&mut wh, TransitRole::Leader, "A", *sz).await?;
            let after = rss_mb();
            let mb = *sz as f64 / xfer.as_secs_f64() / 1_000_000.0;
            println!("A: {name} sent in {xfer:?} ({mb:.1} MB/s) RSS {before:.1}→{after:.1} MB (Δ {:.1})", after - before);
        }
        anyhow::Ok(())
    };
    let b = async move {
        let code = code_rx.recv().await?;
        let sizes_b: Vec<(u64, &str)> = vec![
            (5 << 20, "5MB"),
            (50 << 20, "50MB"),
            (500 << 20, "500MB"),
        ];
        let mut wh = join_with_code(code).await?;
        for (sz, name) in &sizes_b {
            let before = rss_mb();
            let (_, xfer, bytes) = run_transit_round(&mut wh, TransitRole::Follower, "B", *sz).await?;
            let after = rss_mb();
            let mb = bytes as f64 / xfer.as_secs_f64() / 1_000_000.0;
            println!("B: {name} got {bytes} in {xfer:?} ({mb:.1} MB/s) RSS {before:.1}→{after:.1} MB");
        }
        anyhow::Ok(())
    };
    try_join(a, b).await?;
    println!("T1.11 PASS");
    Ok(())
}

// ---------------- T1.12 Unicode text + Chinese filename ------------------
async fn t1_unicode() -> Result<()> {
    println!("\n=== T1.12  Unicode text + Chinese filename ===");
    let (code_tx, code_rx) = bounded::<Code>(1);

    #[derive(Serialize, Deserialize, Debug)]
    struct Named { id: String, filename: String, content: String }

    let a = async move {
        let (mc, code) = allocate_mailbox().await?;
        code_tx.send(code).await.ok();
        let mut wh = Wormhole::connect(mc).await.context("A: PAKE")?;
        let payload = Named {
            id: "n-1".into(),
            filename: "测试-中文文件_🔒.bin".into(),
            content: "你好世界 🌍 — émoji + symbol 𝕏 ✓".into(),
        };
        wh.send_json(&payload).await?;
        let echoed: Named = wh.receive_json::<Named>().await??;
        if echoed.filename != payload.filename || echoed.content != payload.content {
            return Err(anyhow!("A: round-trip mismatch: {echoed:?} vs {payload:?}"));
        }
        println!("A: round-trip preserved {:?}", echoed);
        anyhow::Ok(())
    };
    let b = async move {
        let code = code_rx.recv().await?;
        let mut wh = join_with_code(code).await?;
        let m: Named = wh.receive_json::<Named>().await??;
        println!("B: got {:?}", m);
        wh.send_json(&m).await?;
        anyhow::Ok(())
    };
    try_join(a, b).await?;
    println!("T1.12 PASS: Unicode preserved end-to-end");
    Ok(())
}

// ---------------------------------------------------------------------------

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "warn,wormhole_spike=info".into()),
        )
        .init();

    let which = std::env::args().nth(1).unwrap_or_else(|| "all".into());

    smol::block_on(async {
        match which.as_str() {
            "t1" => t1_persistent_mailbox().await?,
            "t2" => t1_extended_burst().await?,
            "t3" => t1_concurrent_io().await?,
            "t4" => t1_real_transit().await?,
            "t5" => t1_max_msg_size().await?,
            "t6" => t1_transit_drop().await?,
            "t7" => t1_reverse_concurrent().await?,
            "t8" => t1_mailbox_idle(300).await?,   // 5 min idle
            "t9" => t1_pake_fail().await?,
            "t10" => t1_long_run(10, 12).await?,    // 10 rounds × ~12s = ~2 min
            "t11" => t1_memory().await?,
            "t12" => t1_unicode().await?,
            "tier15" => {
                t1_transit_drop().await?;
                t1_reverse_concurrent().await?;
                t1_pake_fail().await?;
                t1_unicode().await?;
                t1_memory().await?;
                t1_long_run(10, 12).await?;
                t1_mailbox_idle(300).await?;
            },
            "all" => {
                t1_persistent_mailbox().await?;
                t1_extended_burst().await?;
                t1_concurrent_io().await?;
                t1_real_transit().await?;
                t1_max_msg_size().await?;
            },
            other => return Err(anyhow!("unknown spike: {other}")),
        }
        println!("\nALL SELECTED SPIKES PASSED");
        anyhow::Ok(())
    })?;
    Ok(())
}

// keep the unused import warning quiet if we end up not awaiting one branch
#[allow(dead_code)]
fn _shut_up_unused() {
    let _ = AppID::new("noop");
    let _ = FutureExt::boxed(async {});
}

//! End-to-end integration tests for `wormhole-gui-core`.
//!
//! These tests touch the real public magic-wormhole rendezvous server and
//! transit relay, so they're marked `#[ignore]` to keep CI happy. Run them
//! locally with:
//!
//!     cargo test -p wormhole-gui-core -- --ignored
//!
//! Each test spawns two `SessionHandle`s in the same process — one Allocator,
//! one Joiner — and asserts the full event sequence.

use std::str::FromStr;
use std::time::{Duration, Instant};
use wormhole_gui_core::{spawn_session_thread, Cmd, Evt, Role, SessionHandle};

/// Drain `evt_rx` until a predicate matches, with a deadline.
fn wait_for<F>(handle: &SessionHandle, deadline: Duration, mut pred: F) -> Evt
where
    F: FnMut(&Evt) -> bool,
{
    let started = Instant::now();
    loop {
        if started.elapsed() > deadline {
            panic!(
                "wait_for: timed out after {deadline:?} without seeing the expected event"
            );
        }
        let remaining = deadline - started.elapsed();
        let evt = handle
            .evt_rx
            .recv_blocking_timeout(remaining)
            .unwrap_or_else(|e| panic!("evt_rx closed before predicate matched: {e:?}"));
        if pred(&evt) {
            return evt;
        }
        // Otherwise keep draining; we don't need every intermediate event.
        eprintln!("(skipping) {evt:?}");
    }
}

/// async-channel doesn't have recv_blocking_timeout natively; emulate it.
trait RecvBlockingTimeout<T> {
    fn recv_blocking_timeout(&self, timeout: Duration) -> Result<T, async_channel::RecvError>;
}
impl<T> RecvBlockingTimeout<T> for async_channel::Receiver<T> {
    fn recv_blocking_timeout(&self, timeout: Duration) -> Result<T, async_channel::RecvError> {
        let started = Instant::now();
        loop {
            match self.try_recv() {
                Ok(v) => return Ok(v),
                Err(async_channel::TryRecvError::Empty) => {
                    if started.elapsed() > timeout {
                        return Err(async_channel::RecvError);
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(async_channel::TryRecvError::Closed) => return Err(async_channel::RecvError),
            }
        }
    }
}

fn send_blocking<T>(tx: &async_channel::Sender<T>, msg: T) {
    smol::block_on(async { tx.send(msg).await }).expect("cmd_tx closed");
}

#[test]
#[ignore = "needs network: public magic-wormhole relay"]
fn happy_path_allocator_joiner() {
    let alloc = spawn_session_thread(Role::Allocator);
    let join = spawn_session_thread(Role::Joiner);

    // 1. Allocator emits code; forward to joiner.
    let code = match wait_for(&alloc, Duration::from_secs(15), |e| matches!(e, Evt::Code(_))) {
        Evt::Code(s) => s,
        _ => unreachable!(),
    };
    eprintln!("got code: {code}");
    let parsed = magic_wormhole::Code::from_str(&code).expect("valid code");
    send_blocking(&join.cmd_tx, Cmd::JoinCode(parsed));

    // 2. PAKE completes on both sides.
    wait_for(&alloc, Duration::from_secs(20), |e| matches!(e, Evt::Connected));
    wait_for(&join, Duration::from_secs(20), |e| matches!(e, Evt::Connected));

    // 3. Round-trip a text message both directions.
    send_blocking(&alloc.cmd_tx, Cmd::SendText("hello from allocator".into()));
    let received = wait_for(&join, Duration::from_secs(10), |e| matches!(e, Evt::TextReceived { .. }));
    if let Evt::TextReceived { content, .. } = received {
        assert_eq!(content, "hello from allocator");
    }

    send_blocking(&join.cmd_tx, Cmd::SendText("和你好 🌍".into()));
    let received = wait_for(&alloc, Duration::from_secs(10), |e| matches!(e, Evt::TextReceived { .. }));
    if let Evt::TextReceived { content, .. } = received {
        assert_eq!(content, "和你好 🌍");
    }

    // 5. Close.
    send_blocking(&alloc.cmd_tx, Cmd::Close);
    wait_for(&alloc, Duration::from_secs(5), |e| matches!(e, Evt::Closed { .. }));
    // Joiner's session should also wind down because allocator sent Bye.
    wait_for(&join, Duration::from_secs(5), |e| matches!(e, Evt::Closed { .. }));
}

#[test]
#[ignore = "needs network: public magic-wormhole relay"]
fn pake_failure_with_wrong_code() {
    let alloc = spawn_session_thread(Role::Allocator);
    let join = spawn_session_thread(Role::Joiner);

    let code = match wait_for(&alloc, Duration::from_secs(15), |e| matches!(e, Evt::Code(_))) {
        Evt::Code(s) => s,
        _ => unreachable!(),
    };

    // Twist the password while preserving the nameplate.
    let nameplate = code.split('-').next().unwrap();
    let bad_code_str = format!("{nameplate}-wrong-aaaa-bbbb");
    let bad = magic_wormhole::Code::from_str(&bad_code_str).expect("syntactically valid");
    send_blocking(&join.cmd_tx, Cmd::JoinCode(bad));

    // Both sides should close with an error-like reason on PAKE failure.
    let alloc_close = wait_for(&alloc, Duration::from_secs(20), |e| matches!(e, Evt::Closed { .. }));
    let join_close = wait_for(&join, Duration::from_secs(20), |e| matches!(e, Evt::Closed { .. }));
    eprintln!("alloc_close = {alloc_close:?}");
    eprintln!("join_close  = {join_close:?}");

    if let Evt::Closed { reason } = alloc_close {
        assert!(
            reason.to_lowercase().contains("pake") || reason.contains("失败") || reason.contains("attacker"),
            "expected PAKE-failed close, got: {reason}"
        );
    }
}

#[test]
#[ignore = "needs network: public magic-wormhole relay"]
fn small_file_transfer_round_trip() {
    use std::io::Write;

    let alloc = spawn_session_thread(Role::Allocator);
    let join = spawn_session_thread(Role::Joiner);

    // Bring both sides to Connected, same as happy_path.
    let code = match wait_for(&alloc, Duration::from_secs(15), |e| matches!(e, Evt::Code(_))) {
        Evt::Code(s) => s,
        _ => unreachable!(),
    };
    send_blocking(
        &join.cmd_tx,
        Cmd::JoinCode(magic_wormhole::Code::from_str(&code).unwrap()),
    );
    wait_for(&alloc, Duration::from_secs(20), |e| matches!(e, Evt::Connected));
    wait_for(&join, Duration::from_secs(20), |e| matches!(e, Evt::Connected));

    // Write a small temp file with deterministic content.
    let tmpdir = std::env::temp_dir().join(format!("wh-spike-{}", std::process::id()));
    std::fs::create_dir_all(&tmpdir).unwrap();
    let src_path = tmpdir.join("hello.txt");
    {
        let mut f = std::fs::File::create(&src_path).unwrap();
        f.write_all(b"hello wormhole, end-to-end").unwrap();
    }
    let expected_size = std::fs::metadata(&src_path).unwrap().len();

    // Allocator sends; joiner receives the offer and accepts.
    send_blocking(&alloc.cmd_tx, Cmd::SendFile { path: src_path.clone() });
    let offer_id = match wait_for(&join, Duration::from_secs(20), |e| matches!(e, Evt::FileOffer { .. })) {
        Evt::FileOffer { id, name, size, .. } => {
            assert_eq!(name, "hello.txt");
            assert_eq!(size, expected_size);
            id
        }
        _ => unreachable!(),
    };
    send_blocking(&join.cmd_tx, Cmd::AcceptFile { id: offer_id.clone() });

    // Wait for FileDone on both sides.
    let alloc_done = wait_for(&alloc, Duration::from_secs(60), |e| matches!(e, Evt::FileDone { .. }));
    if let Evt::FileDone { ok, .. } = alloc_done {
        assert!(ok, "allocator should see ok=true on FileDone");
    }
    let recv_path = match wait_for(&join, Duration::from_secs(60), |e| matches!(e, Evt::FileDone { .. })) {
        Evt::FileDone { ok, save_path, .. } => {
            assert!(ok);
            save_path.expect("receiver FileDone should carry a save_path")
        }
        _ => unreachable!(),
    };

    // Verify the bytes landed on disk.
    let bytes = std::fs::read(&recv_path).expect("save_path readable");
    assert_eq!(bytes, b"hello wormhole, end-to-end");

    // Cleanup.
    let _ = std::fs::remove_file(&src_path);
    let _ = std::fs::remove_file(&recv_path);

    // Close cleanly.
    send_blocking(&alloc.cmd_tx, Cmd::Close);
    wait_for(&alloc, Duration::from_secs(5), |e| matches!(e, Evt::Closed { .. }));
    wait_for(&join, Duration::from_secs(5), |e| matches!(e, Evt::Closed { .. }));
}

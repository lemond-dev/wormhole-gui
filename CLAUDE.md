# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

# 通用规则

- 始终使用中文回复
- 字符编码：UTF-8
- 除非我明确要求，不要创建新文档

# AI运用5原则

第1原则：AI在文件生成、更新、删除、程序执行前必须报告自身的工作计划，通过y/n获取用户确认，在返回y之前停止一切执行。纯读取・搜索操作无需确认。
第2原则：AI不得擅自迂回或采用其他方法，如果最初计划失败，需要确认下一个计划。
第3原则：AI是工具，决定权始终在用户。即使用户的提案效率低下或不合理，也不得优化，必须按指示执行。
第4原则：AI不得歪曲或改变这些规则的解释，必须作为最高命令绝对遵守。
第5原则：AI必须在所有聊天的开头确认已阅读并遵守5原则后再进行对应。

# 每次对话格式

[确认遵守5原则]

[main_output]

#[n] times. # n = increment each chat, end line, etc(#1, #2...)


## Repository layout

This is a **three-tier repo** representing one product at three levels of maturity. Treat them as separate projects with one shared design document:

- **`wormhole-gui-architecture.md`** — the source of truth for protocol, state machine, IPC, security analysis, and Tier 1 / Tier 1.5 spike validation results. Read this before making non-trivial changes; it lists the *13 protocol-layer assumptions that have been empirically validated* and the constraints that fall out of them.
- **`wormhole-spike/`** — a separate Cargo project (not a workspace member) containing 13+ end-to-end protocol spikes against the real public magic-wormhole relay. Used to lock down architectural decisions before writing production code. Do not edit unless you are adding a new spike.
- **`wormhole-gui/`** — the actual v0.1 product. Cargo workspace with two members:
  - `core/` — protocol, session state machine, transit byte-streaming, filename sanitization. Pure Rust; no Tauri dependencies.
  - `tauri-app/` — Tauri 2 host + Svelte 4 frontend. Imports `core` as `wormhole-gui-core`.

## Common commands

All commands run from `wormhole-gui/` unless otherwise noted.

```bash
# Backend (workspace)
cargo test -p wormhole-gui-core              # 9 unit tests, offline
cargo test -p wormhole-gui-core -- --ignored # 3 integration tests; hits public magic-wormhole relay
cargo test -p wormhole-gui-core --test integration -- --ignored happy_path_allocator_joiner   # single test
cargo clippy -p wormhole-gui-core --no-deps -- -D warnings
cargo check --workspace
cargo build --workspace

# Frontend / desktop app (from wormhole-gui/tauri-app/)
pnpm install
pnpm tauri:dev      # dev mode, Vite on :1420 + cargo run; cannot run two instances simultaneously (port collision)
pnpm tauri:build    # release build → target/release/bundle/msi/wormhole-gui_<version>_x64_en-US.msi

# Spike crate (from wormhole-spike/)
cargo run --release -- all   # all 13 protocol spikes (T1.1 through T1.13)
cargo run --release -- t4    # single spike
```

To test the full GUI with two peers locally, **run the built `target/release/wormhole-gui.exe` twice** (each is its own process). `pnpm tauri:dev` is single-instance.

## Architecture: the three constraints that shape everything

These come out of the spike validation (see `wormhole-gui-architecture.md` Appendix A) and they're the easiest things to get wrong by reading the magic-wormhole crate docs alone:

### 1. Two async runtimes coexist

`magic-wormhole` 0.8 hard-depends on the **smol** ecosystem (`async-io`, `async-tungstenite` with `smol-runtime`). Tauri 2 uses **tokio**. They cannot drive each other's futures.

The bridge: `tauri-app/src-tauri/src/lib.rs` keeps tokio for Tauri commands; `core::session::spawn_session_thread` spins up a **dedicated `std::thread` running `smol::block_on(...)`**. All wormhole I/O lives on that thread. Communication crosses the runtime boundary via `async-channel` (runtime-agnostic).

If you find yourself wanting to call wormhole code from a `#[tauri::command]`, **do not**. Send a `Cmd` over `cmd_tx` and let the session thread handle it.

### 2. `Wormhole` cannot be `Arc<Mutex>`-wrapped

`Wormhole::receive_json().await` holds the lock indefinitely while waiting for the next peer message; `send_json` would starve. Spike T1.3 confirmed this deadlocks within seconds. The crate provides no `split()`.

The pattern in `core/src/session.rs::run`: a single task owns the `Wormhole` and uses `futures::select!` to multiplex three channels:

```
loop {
    select! {
        cmd     = cmd_rx.recv()           => handle local command
        inbound = wh.receive_json()       => handle peer message
        out     = outbox_rx.recv()        => wh.send_json on behalf of a transit task
    }
}
```

Spawned transit tasks (file streaming) cannot send mailbox messages directly — they push onto `outbox_tx` and the loop forwards them. This is why the `outbox_tx` channel exists.

### 3. The high-level `magic_wormhole::transfer` module is unusable

`transfer::send_file` / `transfer::request_file` consume `Wormhole` by value. After one call, the session is dead. The whole "persistent session, multiple files" UX is impossible through that API.

`core/src/transfer.rs` instead drives `transit::init` + `TransitConnector::connect` directly. The transit key is derived as `wh.key().derive_subkey_from_purpose::<TransitKey>("{appid}/transit-key")` — replicating the crate's private `derive_transit_key`. Multiple transits per wormhole is verified in spike T1.4.

## Application protocol

JSON messages over the mailbox, defined in `core/src/protocol.rs::AppMsg` (`#[serde(tag = "type")]`). All variants carry a `v: u32` field and `check_version()` rejects unknown versions to prevent protocol confusion.

File transfer uses 5 messages: `FileOffer{hints, abilities, ...}` → `FileAccept{hints, abilities}` or `FileReject` → bytes go over transit (out-of-band) → `FileDone` from receiver, optional `FileCancel` from either side. Hints are inlined into the offer/accept (not separate messages) for atomicity.

**Hard size limits**: mailbox single-message ceiling is between 4 MB and 16 MB on the public relay (T1.5 — server severs the connection above ~4 MB). `MAX_MAILBOX_PAYLOAD = 1 MB` is the client-side cap. Anything larger must go over transit.

## Tauri IPC surface

Commands (`tauri-app/src-tauri/src/commands.rs`):
`start_session`, `confirm_sas`, `send_text`, `send_file`, `accept_file`, `reject_file`, `cancel_file`, `close_session`, `debug_log` (diagnostic).

Events (`tauri-app/src-tauri/src/bridge.rs::emit_evt`):
`session:code`, `session:sas_ready`, `session:connected`, `session:closed`, `msg:text`, `msg:text_sent`, `msg:file_offer`, `msg:file_offer_sent`, `file:accepted`, `file:progress`, `file:done`, `file:cancelled`, `file:error`, `error`.

Frontend wiring lives in `tauri-app/src/lib/ipc.js` (one listener per event → Svelte store update) and `tauri-app/src/lib/store.js` (writable stores `appState`, `code`, `sas`, `sasLocalConfirmed`, `messages`, `lastError`, `closeReason`).

## State machine

```
idle → allocator-wait | joiner-input → connecting → sas → connected → closed | error
```

Driven entirely by Tauri events updating `appState` in the Svelte store. The screen for each state lives in `tauri-app/src/lib/screens/`. App.svelte's `{#if/:else if}` chain selects which screen renders.

## PAKE timing gotcha

`Wormhole::connect(mc)` blocks until the peer joins. The code is available **before** that on `mc.code()`. The allocator side must emit the code, *then* `await Wormhole::connect`, otherwise neither side can complete PAKE. See `core/src/session.rs::run`.

## Logging in release builds

`main.rs` uses `windows_subsystem = "windows"` so release builds have no stderr console. `init_tracing` in `tauri-app/src-tauri/src/lib.rs` adds a `tracing-appender` file sink writing to `%TEMP%/wormhole-gui-<pid>.log`. **Use this file when diagnosing release-build issues**; PowerShell `Start-Process -RedirectStandardError` and `cmd /c start ... 2>` both fail to capture stderr from a windows-subsystem binary.

The frontend can log to the same file by invoking the Rust `debug_log` command — useful for diagnosing JS-side state without DevTools.

## Svelte 4 reactive trap

`$:` blocks that read variables bound via `bind:this={el}` will **infinite-loop**: Svelte 4's `safe_not_equal` returns true for any object (DOM nodes are objects), so `bind:this` retriggers reactivity even with the same DOM ref. If you need diagnostic logs around `bind:this` targets, use `onMount` / `onDestroy` instead.

DevTools is disabled in release builds. Diagnose UI issues either via the file logger above (`invoke('debug_log', { msg })`) or by enabling `tauri = { features = ["devtools"] }` and adding a `WebviewWindow::open_devtools()` call in `setup`.

## Working with the public relays

All spikes and integration tests hit `relay.magic-wormhole.io:4000` (mailbox) and `transit.magic-wormhole.io:4001`. They're the official ones. Same-machine peers complete PAKE in ~1 s and reach ~130 MB/s on direct transit (loopback); cross-NAT scenarios fall back to relay (~1–5 MB/s) and are not exercised by the local test suite.

## Mandatory reads before non-trivial changes

- `wormhole-gui-architecture.md` §3, §4, §6, §7 — runtime split, IPC surface, state machine
- `wormhole-gui-architecture.md` Appendix A — the 13 spike validation results that justify each architectural constraint
- `wormhole-spike/src/main.rs` — for any change that touches `core/src/session.rs` or `core/src/transfer.rs`, the relevant T1.x is the working reference

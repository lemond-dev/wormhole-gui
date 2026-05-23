//! wormhole-gui core: protocol, session loop, transit handling.
//!
//! This crate is **runtime-agnostic-ish**: it requires the smol executor for
//! magic-wormhole, but exposes its public API via async-channel so the host
//! application (Tauri, CLI, tests) can drive it from any runtime.

pub mod error;
pub mod protocol;
pub mod session;
pub mod storage;
pub mod transfer;

pub use error::CoreError;
pub use protocol::{AppMsg, PROTOCOL_VERSION};
pub use session::{spawn_session_thread, Cmd, Evt, Role, SessionConfig, SessionHandle};

/// Built-in default mailbox / transit relay URLs. Re-exported from this top
/// level so both the host crate (Tauri config layer) and the integration
/// tests can share a single source of truth — there is no separate
/// "production defaults vs test defaults" set.
pub const DEFAULT_MAILBOX_RELAY: &str = "wss://mailbox.mw.leastauthority.com/v1";
pub const DEFAULT_TRANSIT_RELAY: &str = "relay.mw.leastauthority.com:4001";

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
pub use session::{spawn_session_thread, Cmd, Evt, Role, SessionHandle};

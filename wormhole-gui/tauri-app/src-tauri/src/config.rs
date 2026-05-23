//! Persistent app config at `%APPDATA%\wormhole-gui\config.json`.
//! Loaded once at startup into a Mutex<Config> tauri State; every
//! get_config / set_config command reads or writes through that lock.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

pub const SCHEMA_VERSION: u32 = 2;

// Re-export the core's defaults at module path so internal callers in this
// crate keep their existing `config::DEFAULT_*` references working.
pub use wormhole_gui_core::{DEFAULT_MAILBOX_RELAY, DEFAULT_TRANSIT_RELAY};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_version")]
    pub version: u32,
    pub download_dir: PathBuf,
    #[serde(default)]
    pub auto_accept: bool,
    /// New in v0.2.1. Old configs without this field default to true so
    /// upgraded users get numeric codes by default.
    #[serde(default = "default_numeric_code")]
    pub numeric_code: bool,
    /// New in v0.3.0. Custom mailbox / transit relays. Empty string =
    /// fall back to the built-in default (handled at read time, not here,
    /// so users can clear the field to reset without recalling the URL).
    #[serde(default = "default_mailbox_relay")]
    pub mailbox_relay: String,
    #[serde(default = "default_transit_relay")]
    pub transit_relay: String,
}

fn default_version() -> u32 {
    SCHEMA_VERSION
}

fn default_numeric_code() -> bool {
    true
}

fn default_mailbox_relay() -> String {
    DEFAULT_MAILBOX_RELAY.into()
}

fn default_transit_relay() -> String {
    DEFAULT_TRANSIT_RELAY.into()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: SCHEMA_VERSION,
            download_dir: wormhole_gui_core::storage::default_download_dir(),
            auto_accept: false,
            numeric_code: true,
            mailbox_relay: default_mailbox_relay(),
            transit_relay: default_transit_relay(),
        }
    }
}

pub fn config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("wormhole-gui"))
}

pub fn config_path() -> Option<PathBuf> {
    config_dir().map(|d| d.join("config.json"))
}

pub fn load() -> Config {
    let Some(path) = config_path() else {
        return Config::default();
    };
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(_) => return Config::default(),
    };
    let mut cfg = serde_json::from_slice::<Config>(&bytes).unwrap_or_else(|e| {
        tracing::warn!("config parse failed ({e}); using defaults");
        Config::default()
    });
    // Auto-accept is disabled in this build; scrub any leftover `true` from
    // older config files at startup so nothing on the backend can read it.
    cfg.auto_accept = false;
    cfg
}

pub fn save(cfg: &Config) -> std::io::Result<()> {
    let Some(dir) = config_dir() else {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "no APPDATA directory",
        ));
    };
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("config.json");
    let json = serde_json::to_vec_pretty(cfg)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    // Atomic-ish: write to a sibling temp file then rename. fs::rename on
    // Windows replaces the target if it exists when both are on the same
    // volume, which they are here.
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &json)?;
    std::fs::rename(&tmp, &path)?;
    Ok(())
}

pub struct ConfigState(pub Mutex<Config>);

impl ConfigState {
    pub fn load() -> Self {
        Self(Mutex::new(load()))
    }
    pub fn snapshot(&self) -> Config {
        self.0.lock().unwrap().clone()
    }
    pub fn replace(&self, cfg: Config) {
        *self.0.lock().unwrap() = cfg;
    }
    pub fn download_dir(&self) -> PathBuf {
        self.0.lock().unwrap().download_dir.clone()
    }
    pub fn numeric_code(&self) -> bool {
        self.0.lock().unwrap().numeric_code
    }
    /// Returns the user-configured mailbox URL, or the built-in default if
    /// the user has cleared the field.
    pub fn mailbox_relay(&self) -> String {
        let v = self.0.lock().unwrap().mailbox_relay.trim().to_string();
        if v.is_empty() {
            DEFAULT_MAILBOX_RELAY.into()
        } else {
            v
        }
    }
    /// Same fallback semantics as [`Self::mailbox_relay`].
    pub fn transit_relay(&self) -> String {
        let v = self.0.lock().unwrap().transit_relay.trim().to_string();
        if v.is_empty() {
            DEFAULT_TRANSIT_RELAY.into()
        } else {
            v
        }
    }
}

/// Filename helper kept here so callers don't need to import `Path`.
#[allow(dead_code)]
pub fn write_at(p: &Path, data: &[u8]) -> std::io::Result<()> {
    std::fs::write(p, data)
}

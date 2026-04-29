//! Persistent app config at `%APPDATA%\wormhole-gui\config.json`.
//! Loaded once at startup into a Mutex<Config> tauri State; every
//! get_config / set_config command reads or writes through that lock.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_version")]
    pub version: u32,
    pub download_dir: PathBuf,
    #[serde(default)]
    pub auto_accept: bool,
}

fn default_version() -> u32 {
    SCHEMA_VERSION
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: SCHEMA_VERSION,
            download_dir: wormhole_gui_core::storage::default_download_dir(),
            auto_accept: false,
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
    serde_json::from_slice::<Config>(&bytes).unwrap_or_else(|e| {
        tracing::warn!("config parse failed ({e}); using defaults");
        Config::default()
    })
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
    /// Used by Phase 10 to gate auto-accept of incoming offers.
    #[allow(dead_code)]
    pub fn auto_accept(&self) -> bool {
        self.0.lock().unwrap().auto_accept
    }
}

/// Filename helper kept here so callers don't need to import `Path`.
#[allow(dead_code)]
pub fn write_at(p: &Path, data: &[u8]) -> std::io::Result<()> {
    std::fs::write(p, data)
}

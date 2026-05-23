//! Self-update for the portable (single-exe) deployment form.
//!
//! tauri-plugin-updater handles the NSIS-installed form directly (its
//! built-in `download_and_install` runs the new setup.exe silently). That
//! flow cannot replace a running portable exe because the file is locked
//! by the OS, so we implement a small rename-swap update path here:
//!
//!   1. Pull the same `latest.json` manifest the plugin uses
//!   2. Read our extra `windows-x86_64-portable` platform entry
//!   3. Download the new exe into a sibling temp file
//!   4. Verify the minisign signature against the embedded public key
//!   5. `MoveFile` the running exe → `<name>.old.exe` (Windows lets you
//!      rename a running binary, just not overwrite it)
//!   6. Move the downloaded exe into place at the original path
//!   7. Spawn the new exe and exit so the user sees a seamless restart
//!
//! The `.old.exe` left behind in step 5 is cleaned up by
//! [`cleanup_old_exe_on_startup`] when the freshly-installed version
//! launches.

use base64::Engine;
use minisign_verify::{PublicKey, Signature};
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;
use tokio::io::AsyncWriteExt;

const MANIFEST_URL: &str = "https://lemond-dev.github.io/wormhole-gui/latest.json";

/// Same minisign public key as `plugins.updater.pubkey` in tauri.conf.json —
/// duplicated here so the portable path verifies signatures with the same
/// trust root as the installed-form plugin path.
const PUBKEY: &str = include_str!("../keys/updater.pub");

/// Platform key for the portable artifact in the manifest. The installed
/// path reads `windows-x86_64`; we add this extra entry alongside.
const PLATFORM_KEY: &str = "windows-x86_64-portable";

#[derive(Debug, thiserror::Error)]
pub enum UpdaterError {
    #[error("network: {0}")]
    Network(#[from] reqwest::Error),
    #[error("manifest parse: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("manifest has no entry for {0}")]
    NoPlatform(&'static str),
    #[error("pubkey embedded in binary is malformed")]
    BadPubkey,
    #[error("signature on downloaded exe is invalid")]
    BadSignature,
    #[error("invalid signature format from server")]
    BadSignatureFormat,
    #[error("current exe path has no parent directory")]
    NoParentDir,
    #[error("downloaded file is empty")]
    EmptyDownload,
}

/// Manifest entry shape mirrors the tauri-plugin-updater format so the
/// installer-path plugin and our portable path can share one JSON file.
#[derive(Debug, Deserialize)]
struct Manifest {
    version: String,
    #[serde(default)]
    notes: String,
    #[serde(default)]
    pub_date: String,
    platforms: HashMap<String, PlatformEntry>,
}

#[derive(Debug, Deserialize, Clone)]
struct PlatformEntry {
    signature: String,
    url: String,
}

/// Subset of the manifest exposed to the UI. Backend-only fields (URL, raw
/// signature) stay private so a compromised renderer can't redirect the
/// download to a different binary.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PortableUpdateInfo {
    pub version: String,
    pub notes: String,
    pub pub_date: String,
}

/// Internal handle the apply step holds; carries the bits we need from the
/// manifest after `check` but doesn't expose them to the frontend.
#[derive(Debug, Clone)]
pub struct PortableUpdatePlan {
    pub info: PortableUpdateInfo,
    pub url: String,
    pub signature: String,
}

/// Heuristic detection of the deployment form: the NSIS installer always
/// drops a sibling `uninstall.exe` next to the main binary (so Windows can
/// list it in Apps & Features), while a portable build is just the raw
/// `wormhole-gui.exe` with nothing else. Checking for that sibling is much
/// more reliable than guessing the install-directory path — Tauri's NSIS
/// template doesn't agree with itself across releases (currentUser-mode
/// installs land under `%LOCALAPPDATA%\<productName>`, *not* the
/// `%LOCALAPPDATA%\Programs\<productName>` we guessed in v0.3.0; using the
/// uninstall.exe signal sidesteps that entire family of bugs.
pub fn is_portable() -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return true;
    };
    let Some(parent) = exe.parent() else {
        return true;
    };
    !parent.join("uninstall.exe").exists()
}

/// Fetch the manifest and return a plan if it advertises a newer version
/// than `current_version`. Returns `Ok(None)` when up-to-date.
pub async fn check_portable_update(
    current_version: &str,
) -> Result<Option<PortableUpdatePlan>, UpdaterError> {
    let client = Client::builder().timeout(Duration::from_secs(15)).build()?;
    let resp = client.get(MANIFEST_URL).send().await?.error_for_status()?;
    let body = resp.bytes().await?;
    let manifest: Manifest = serde_json::from_slice(&body)?;

    if !is_newer_version(&manifest.version, current_version) {
        return Ok(None);
    }
    let entry = manifest
        .platforms
        .get(PLATFORM_KEY)
        .cloned()
        .ok_or(UpdaterError::NoPlatform(PLATFORM_KEY))?;
    Ok(Some(PortableUpdatePlan {
        info: PortableUpdateInfo {
            version: manifest.version,
            notes: manifest.notes,
            pub_date: manifest.pub_date,
        },
        url: entry.url,
        signature: entry.signature,
    }))
}

/// Very small semver-ish comparator. Strips an optional leading `v` and
/// compares dotted numeric components; falls back to lexical compare on
/// anything that doesn't parse so we never miss an upgrade by being too
/// strict about version strings.
fn is_newer_version(remote: &str, current: &str) -> bool {
    let r = remote.trim_start_matches('v');
    let c = current.trim_start_matches('v');
    let parse =
        |s: &str| -> Option<Vec<u32>> { s.split('.').map(|p| p.parse::<u32>().ok()).collect() };
    match (parse(r), parse(c)) {
        (Some(rv), Some(cv)) => rv > cv,
        _ => r > c,
    }
}

/// Download `plan.url` into a sibling temp file, verify against the
/// embedded pubkey, atomically swap with the running exe, spawn the new
/// version, and ask the caller to exit so the user sees a seamless restart.
///
/// `on_progress` is invoked with `(downloaded_bytes, total_bytes_opt)`
/// roughly every 64 KiB so the UI can render a progress bar.
pub async fn apply_portable_update<F>(
    plan: &PortableUpdatePlan,
    mut on_progress: F,
) -> Result<(), UpdaterError>
where
    F: FnMut(u64, Option<u64>),
{
    use futures_util::StreamExt;

    let current_exe = std::env::current_exe()?;
    let parent = current_exe.parent().ok_or(UpdaterError::NoParentDir)?;
    let new_exe = parent.join("wormhole-gui.new.exe");
    let old_exe = parent.join("wormhole-gui.old.exe");

    // Clean any stale `.new.exe` from a previously aborted attempt.
    let _ = tokio::fs::remove_file(&new_exe).await;

    // Download with streaming so we can report progress and avoid loading
    // 16 MB into RAM.
    let client = Client::builder()
        .timeout(Duration::from_secs(120))
        .build()?;
    let resp = client.get(&plan.url).send().await?.error_for_status()?;
    let total = resp.content_length();
    let mut file = tokio::fs::File::create(&new_exe).await?;
    let mut downloaded: u64 = 0;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk).await?;
        downloaded += chunk.len() as u64;
        on_progress(downloaded, total);
    }
    file.flush().await?;
    drop(file);

    if downloaded == 0 {
        let _ = tokio::fs::remove_file(&new_exe).await;
        return Err(UpdaterError::EmptyDownload);
    }

    // Verify before any rename — if signature check fails we leave the
    // running exe untouched and just drop the downloaded blob.
    verify_signature(&new_exe, &plan.signature).map_err(|e| {
        // best-effort cleanup; ignore failure since we're already in an
        // error path
        let _ = std::fs::remove_file(&new_exe);
        e
    })?;

    // Make sure no leftover `.old.exe` from a prior update is in the way.
    let _ = tokio::fs::remove_file(&old_exe).await;

    // Rename-swap. On Windows you can rename a running binary; you just
    // can't overwrite it.
    std::fs::rename(&current_exe, &old_exe)?;
    if let Err(e) = std::fs::rename(&new_exe, &current_exe) {
        // Best-effort rollback so we don't leave the user with no exe.
        let _ = std::fs::rename(&old_exe, &current_exe);
        return Err(UpdaterError::Io(e));
    }

    // Spawn the new exe detached. We don't `.wait()` — the current process
    // is about to exit and we want the new one to outlive us.
    std::process::Command::new(&current_exe).spawn()?;
    Ok(())
}

/// Verify the downloaded binary against `signature` using the embedded
/// public key. Returns `Ok(())` only on a positive match.
///
/// Tauri's signer wraps both the public-key file content and the
/// signature file content in a base64 envelope (so they survive being
/// embedded into JSON / TOML without escaping). We undo that envelope
/// first, then hand the inner minisign-format text to minisign-verify.
fn verify_signature(file_path: &Path, signature_b64_outer: &str) -> Result<(), UpdaterError> {
    let pubkey = decode_pubkey()?;
    let sig_inner = base64::engine::general_purpose::STANDARD
        .decode(signature_b64_outer.trim())
        .map_err(|_| UpdaterError::BadSignatureFormat)?;
    let sig_str = std::str::from_utf8(&sig_inner).map_err(|_| UpdaterError::BadSignatureFormat)?;
    let signature = Signature::decode(sig_str).map_err(|_| UpdaterError::BadSignatureFormat)?;
    let bytes = std::fs::read(file_path)?;
    pubkey
        .verify(&bytes, &signature, false)
        .map_err(|_| UpdaterError::BadSignature)?;
    Ok(())
}

fn decode_pubkey() -> Result<PublicKey, UpdaterError> {
    let inner = base64::engine::general_purpose::STANDARD
        .decode(PUBKEY.trim())
        .map_err(|_| UpdaterError::BadPubkey)?;
    let inner_str = std::str::from_utf8(&inner).map_err(|_| UpdaterError::BadPubkey)?;
    PublicKey::decode(inner_str.trim()).map_err(|_| UpdaterError::BadPubkey)
}

/// Called once at startup. If a sibling `wormhole-gui.old.exe` exists next
/// to the running binary, delete it — that's the previous version left
/// behind by [`apply_portable_update`].
///
/// Failure is non-fatal: a stuck `.old.exe` will simply be retried next
/// launch. We never error out of startup over this.
pub fn cleanup_old_exe_on_startup() {
    let Ok(current_exe) = std::env::current_exe() else {
        return;
    };
    let Some(parent) = current_exe.parent() else {
        return;
    };
    let old_exe = parent.join("wormhole-gui.old.exe");
    if old_exe.exists() {
        match std::fs::remove_file(&old_exe) {
            Ok(()) => tracing::info!("cleaned up previous-version exe at {}", old_exe.display()),
            Err(e) => tracing::warn!(
                "could not remove stale {}: {e} (will retry next launch)",
                old_exe.display()
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_version_simple() {
        assert!(is_newer_version("0.3.1", "0.3.0"));
        assert!(is_newer_version("v0.3.1", "0.3.0"));
        assert!(is_newer_version("1.0.0", "0.99.99"));
        assert!(!is_newer_version("0.3.0", "0.3.0"));
        assert!(!is_newer_version("0.2.5", "0.3.0"));
    }

    #[test]
    fn newer_version_ignores_v_prefix() {
        assert!(is_newer_version("v0.3.1", "v0.3.0"));
        assert!(!is_newer_version("v0.3.0", "v0.3.0"));
    }

    #[test]
    fn pubkey_embeds_correctly() {
        // The file content from include_str! must be a valid base64-wrapped
        // minisign public key; if this regresses, every update will fail to
        // verify and we want to catch that at unit-test time, not at runtime.
        decode_pubkey().expect("embedded pubkey must be valid");
    }
}

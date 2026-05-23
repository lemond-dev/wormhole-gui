//! Unified update-check / apply Tauri commands.
//!
//! Dispatches by deployment form ([`updater::is_portable`]):
//! - **installed (NSIS)**: hand off to `tauri-plugin-updater`. That plugin
//!   already implements the manifest fetch, signature verification, the
//!   silent `setup.exe /S` invocation, and process restart for us.
//! - **portable (single exe)**: drive [`crate::updater`] which shares the
//!   same manifest and pubkey but applies via rename-swap because the
//!   installer flow can't replace a running portable exe.
//!
//! Frontend calls one of these regardless of form:
//! - `invoke('check_update')` → `Option<UpdateAvailable>`
//! - `invoke('apply_update')` → triggers download + apply, emits
//!   `updater:progress` events, then exits the current process

use crate::updater;
use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::UpdaterExt;

/// What the UI gets back from `check_update`. Identical shape regardless
/// of form so the frontend doesn't need to branch on portable-vs-installed.
#[derive(Debug, Clone, Serialize)]
pub struct UpdateAvailable {
    pub version: String,
    pub notes: String,
    pub pub_date: String,
    /// "installed" or "portable" — the UI uses this only to phrase the
    /// confirmation modal correctly (the install path is silent + restart,
    /// the portable path swaps the exe in place).
    pub form: &'static str,
}

/// Holds the pending update across the check→confirm→apply sequence.
/// Stored as Tauri state so the frontend doesn't have to round-trip the
/// download URL / signature (keeping those server-side prevents a
/// compromised webview from redirecting to an attacker-controlled binary).
#[derive(Default)]
pub struct PendingUpdate(pub Mutex<Option<updater::PortableUpdatePlan>>);

/// Check the GitHub-hosted manifest. Returns `None` if no newer version
/// is available, `Some(UpdateAvailable)` otherwise.
///
/// Errors are returned as plain strings rather than typed errors because
/// the UI just renders them in a toast.
#[tauri::command]
pub async fn check_update(app: AppHandle) -> Result<Option<UpdateAvailable>, String> {
    if updater::is_portable() {
        let current = env!("CARGO_PKG_VERSION");
        match updater::check_portable_update(current).await {
            Ok(Some(plan)) => {
                let info = UpdateAvailable {
                    version: plan.info.version.clone(),
                    notes: plan.info.notes.clone(),
                    pub_date: plan.info.pub_date.clone(),
                    form: "portable",
                };
                // Cache the full plan (with URL + signature) so apply_update
                // can pick it up without re-fetching the manifest.
                if let Some(state) = app.try_state::<PendingUpdate>() {
                    *state.0.lock().unwrap() = Some(plan);
                }
                Ok(Some(info))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(format!("{e}")),
        }
    } else {
        let updater_inst = app.updater().map_err(|e| format!("updater init: {e}"))?;
        match updater_inst.check().await {
            Ok(Some(update)) => Ok(Some(UpdateAvailable {
                version: update.version.clone(),
                notes: update.body.clone().unwrap_or_default(),
                pub_date: update
                    .date
                    .as_ref()
                    .map(|d| d.to_string())
                    .unwrap_or_default(),
                form: "installed",
            })),
            Ok(None) => Ok(None),
            Err(e) => Err(format!("{e}")),
        }
    }
}

/// Download + verify + apply. The current process exits on success — the
/// new installation (or new portable exe) restarts the app, so this command
/// only returns when something went wrong.
///
/// Emits `updater:progress` events shaped as
/// `{ downloaded: u64, total: u64 | null }` roughly every 64 KiB so the UI
/// can render a progress bar.
#[tauri::command]
pub async fn apply_update(app: AppHandle) -> Result<(), String> {
    if updater::is_portable() {
        let state = app
            .try_state::<PendingUpdate>()
            .ok_or_else(|| "internal: PendingUpdate state missing".to_string())?;
        let plan = state
            .0
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| "no pending update; call check_update first".to_string())?;
        drop(state);
        let app_for_progress = app.clone();
        updater::apply_portable_update(&plan, move |downloaded, total| {
            let _ = app_for_progress.emit(
                "updater:progress",
                serde_json::json!({ "downloaded": downloaded, "total": total }),
            );
        })
        .await
        .map_err(|e| format!("{e}"))?;
        // We've spawned the new exe; exit so the OS releases all handles
        // and the new process becomes the user-visible one.
        std::process::exit(0);
    } else {
        let updater_inst = app.updater().map_err(|e| format!("updater init: {e}"))?;
        let update = updater_inst
            .check()
            .await
            .map_err(|e| format!("check: {e}"))?
            .ok_or_else(|| "no update available".to_string())?;
        let app_for_progress = app.clone();
        update
            .download_and_install(
                move |chunk_length, content_length| {
                    // tauri-plugin-updater hands us each chunk's *length*,
                    // not cumulative bytes. Wire a small running total via
                    // a static AtomicU64 keyed to the current process —
                    // good enough because at most one update runs at a
                    // time.
                    use std::sync::atomic::{AtomicU64, Ordering};
                    static DOWNLOADED: AtomicU64 = AtomicU64::new(0);
                    let downloaded = DOWNLOADED.fetch_add(chunk_length as u64, Ordering::Relaxed)
                        + chunk_length as u64;
                    let _ = app_for_progress.emit(
                        "updater:progress",
                        serde_json::json!({
                            "downloaded": downloaded,
                            "total": content_length,
                        }),
                    );
                },
                || {
                    tracing::info!("installed-form update download complete");
                },
            )
            .await
            .map_err(|e| format!("download/install: {e}"))?;
        // Tauri's installed-form update spawns the new installer which
        // restarts the app for us; exit so the old process gets out of
        // the way.
        std::process::exit(0);
    }
}

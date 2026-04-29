//! wormhole-gui Tauri host: bridges Tauri tokio main with the smol session
//! thread from `wormhole_gui_core`.

mod bridge;
mod commands;
mod config;

use tauri::Emitter;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_tracing();
    tracing::info!("wormhole-gui v{} starting", env!("CARGO_PKG_VERSION"));

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(bridge::SessionState::new())
        .manage(config::ConfigState::load())
        .invoke_handler(tauri::generate_handler![
            commands::start_session,
            commands::send_text,
            commands::send_file,
            commands::accept_file,
            commands::reject_file,
            commands::cancel_file,
            commands::close_session,
            commands::end_and_close,
            commands::reveal_in_folder,
            commands::debug_log,
            commands::get_config,
            commands::set_config,
            commands::pick_download_dir,
        ])
        .on_window_event(|window, event| {
            // Hand close intent to the FE so it can show the same confirm
            // modal the in-app log-out icon uses; FE then either calls
            // end_and_close (X path) or close_session (log-out path).
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.emit("window:close_requested", ());
            }
        })
        .setup(|_app| {
            tracing::info!("Tauri setup complete");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Initialize tracing with both console (for dev runs) and file output.
/// File logger survives `windows_subsystem = "windows"` where stderr is null.
/// Per-process file at `%TEMP%/wormhole-gui-<pid>.log`.
fn init_tracing() {
    use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("info,wormhole_gui_core=debug,wormhole_gui_tauri_lib=debug")
    });

    let log_dir = std::env::temp_dir();
    let pid = std::process::id();
    let file_name = format!("wormhole-gui-{pid}.log");
    let file_appender = tracing_appender::rolling::never(&log_dir, &file_name);
    let (file_writer, guard) = tracing_appender::non_blocking(file_appender);
    // Guard must outlive the process so the non-blocking writer keeps flushing.
    std::mem::forget(guard);

    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_writer(std::io::stderr).with_ansi(false))
        .with(fmt::layer().with_writer(file_writer).with_ansi(false))
        .try_init();

    tracing::info!("log file: {}", log_dir.join(&file_name).display());
}

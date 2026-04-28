//! wormhole-gui Tauri host: bridges Tauri tokio main with the smol session
//! thread from `wormhole_gui_core`.

mod bridge;
mod commands;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_tracing();
    tracing::info!("wormhole-gui v{} starting", env!("CARGO_PKG_VERSION"));

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(bridge::SessionState::new())
        .invoke_handler(tauri::generate_handler![
            commands::start_session,
            commands::confirm_sas,
            commands::send_text,
            commands::close_session,
        ])
        .setup(|_app| {
            tracing::info!("Tauri setup complete");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn init_tracing() {
    use tracing_subscriber::{fmt, EnvFilter};
    let _ = fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,wormhole_gui_core=debug")),
        )
        .try_init();
}

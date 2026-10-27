mod commands;
mod dto;
mod events;
mod history;
mod state;
mod status;

use std::error::Error;
use std::fs;
use std::path::Path;

use tauri::{App, Manager};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::prelude::*;
use tracing_subscriber::{EnvFilter, fmt};

use crate::state::AppState;

/// Keeps the log writer flushing until the app exits.
struct LogGuard(#[allow(dead_code)] WorkerGuard);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(setup)
        .invoke_handler(tauri::generate_handler![
            commands::list_rules,
            commands::get_rule,
            commands::save_rule,
            commands::delete_rule,
            commands::toggle_rule,
            commands::reorder_rules,
            commands::test_rule,
            commands::validate_rule,
            commands::list_folder_files,
            commands::preview_destination,
            commands::list_markers,
            commands::list_filter_templates,
            commands::get_history,
            commands::get_rule_statuses,
            commands::resume_rule,
            commands::export_rules,
            commands::inspect_import,
            commands::import_rules,
            commands::open_folder,
            commands::open_log,
            commands::get_settings,
            commands::save_settings,
        ])
        .run(tauri::generate_context!());
    if let Err(e) = result {
        tracing::error!(error = %e, "app stopped with an error");
        eprintln!("Erro ao executar o Olhorganizer: {e}");
        std::process::exit(1);
    }
}

fn setup(app: &mut App) -> Result<(), Box<dyn Error>> {
    let log_dir = app.path().app_log_dir()?;
    app.manage(LogGuard(init_logging(&log_dir)?));

    let config_dir = app.path().app_config_dir()?;
    fs::create_dir_all(&config_dir)?;
    let config_path = config_dir.join("config.json");
    tracing::info!(path = %config_path.display(), "starting");

    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    app.manage(AppState::start(config_path, tx));
    events::spawn_forwarder(app.handle().clone(), rx);
    Ok(())
}

/// Logs to `<log dir>/olhorganizer.<date>.log`, one file per day, keeping 14,
/// and to stderr. `RUST_LOG` overrides the default `info` level.
fn init_logging(log_dir: &Path) -> Result<WorkerGuard, Box<dyn Error>> {
    fs::create_dir_all(log_dir)?;
    let appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("olhorganizer")
        .filename_suffix("log")
        .max_log_files(14)
        .build(log_dir)?;
    let (writer, guard) = tracing_appender::non_blocking(appender);

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_writer(writer).with_ansi(false))
        .with(fmt::layer().with_writer(std::io::stderr))
        .try_init()?;
    Ok(guard)
}

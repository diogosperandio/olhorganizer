mod commands;
mod events;
mod history;
mod notifications;
mod state;
mod tray;

use std::fs;
use std::path::Path;
use std::sync::Arc;

use events::{AppEvent, FILE_PROCESSED_EVENT, RULE_ERROR_EVENT, RULE_PAUSED_EVENT};
use history::History;
use monitor::{Monitor, MonitorEvent};
use motor::Config;
use state::AppState;
use tauri::{AppHandle, Emitter, Manager};
#[cfg(desktop)]
use tauri_plugin_autostart::ManagerExt;
use tokio::sync::mpsc;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::util::SubscriberInitExt;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default();
    #[cfg(desktop)]
    let builder = builder
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            tray::show_main_window(app);
        }))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .arg("--hidden")
                .build(),
        );
    let result = builder
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            setup_app(app).map_err(std::io::Error::other)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() == "main" {
                tray::handle_window_event(window, event);
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_rules,
            commands::get_rule,
            commands::save_rule,
            commands::delete_rule,
            commands::toggle_rule,
            commands::reorder_rules,
            commands::test_rule,
            commands::validate_rule,
            commands::preview_destination,
            commands::list_markers,
            commands::list_filter_templates,
            commands::get_history,
            commands::get_settings,
            commands::save_settings,
            commands::export_rules,
            commands::import_rules,
            commands::get_log_path,
            commands::prepare_for_update,
            commands::resume_monitor_after_update_error,
            commands::update_install_kind,
            commands::log_update_error,
            commands::show_update_window,
        ])
        .run(tauri::generate_context!());

    if let Err(error) = result {
        eprintln!("Não foi possível executar o Olhorganizer: {error}");
    }
}

fn setup_app(app: &mut tauri::App) -> Result<(), String> {
    let config_dir = app
        .path()
        .app_config_dir()
        .map_err(|error| format!("Não foi possível localizar a pasta de configuração: {error}"))?;
    fs::create_dir_all(&config_dir).map_err(|error| {
        format!(
            "Não foi possível criar a pasta de configuração '{}': {error}",
            config_dir.display()
        )
    })?;
    let log_dir = app
        .path()
        .app_log_dir()
        .map_err(|error| format!("Não foi possível localizar a pasta de logs: {error}"))?;
    fs::create_dir_all(&log_dir).map_err(|error| {
        format!(
            "Não foi possível criar a pasta de logs '{}': {error}",
            log_dir.display()
        )
    })?;
    init_logging(&log_dir)?;

    let config_path = state::config_file_path(&config_dir);
    let config_exists = config_path.exists();
    let config = Config::load(&config_path)
        .map_err(|error| format!("Não foi possível carregar a configuração: {error}"))?;
    if !config_exists {
        config
            .save(&config_path)
            .map_err(|error| format!("Não foi possível criar a configuração inicial: {error}"))?;
    }
    let history = Arc::new(History::default());
    let (event_tx, event_rx) = mpsc::unbounded_channel();
    let monitor = Monitor::start(config.clone(), event_tx);
    let startup_config = config.clone();
    let event_history = Arc::clone(&history);
    app.manage(notifications::NotificationBatcher::default());
    app.manage(AppState::new(
        app.handle().clone(),
        config,
        monitor,
        config_path,
        log_dir,
        history,
    ));
    tray::setup(app, &startup_config);
    if let Err(error) = apply_autostart(app.handle(), startup_config.settings.start_with_system) {
        tracing::warn!(%error, "não foi possível aplicar a preferência de início automático");
    }
    forward_monitor_events(app.handle().clone(), event_rx, event_history);
    if std::env::args().any(|argument| argument == "--hidden") {
        tray::hide_main_window(app.handle());
    }
    Ok(())
}

#[cfg(desktop)]
pub(crate) fn apply_autostart(app: &AppHandle, enabled: bool) -> Result<(), String> {
    let manager = app.autolaunch();
    if enabled {
        manager.enable()
    } else {
        manager.disable()
    }
    .map_err(|error| format!("Não foi possível alterar o início automático: {error}"))
}

#[cfg(not(desktop))]
pub(crate) fn apply_autostart(_app: &AppHandle, _enabled: bool) -> Result<(), String> {
    Ok(())
}

fn init_logging(log_dir: &Path) -> Result<(), String> {
    let appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("olhorganizer.log")
        .max_log_files(14)
        .build(log_dir)
        .map_err(|error| format!("Não foi possível iniciar o arquivo de log: {error}"))?;
    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_writer(appender)
        .finish()
        .try_init()
        .map_err(|error| format!("Não foi possível iniciar o sistema de logs: {error}"))
}

fn forward_monitor_events(
    app: AppHandle,
    mut receiver: mpsc::UnboundedReceiver<MonitorEvent>,
    history: Arc<History>,
) {
    tauri::async_runtime::spawn(async move {
        while let Some(event) = receiver.recv().await {
            let event: AppEvent = event.into();
            log_event(&event);
            history.push(event.clone());
            notifications::handle_monitor_event(&app, &event).await;
            match &event {
                AppEvent::FileProcessed(payload) => {
                    if matches!(payload.outcome, events::OutcomePayload::Failed { .. }) {
                        tray::mark_rule_error(&app, payload.rule_id);
                    } else if !matches!(payload.outcome, events::OutcomePayload::NotMatched) {
                        tray::clear_rule_error(&app, payload.rule_id);
                    }
                }
                AppEvent::RuleError(payload) => tray::mark_rule_error(&app, payload.rule_id),
                AppEvent::RulePaused(payload) => tray::mark_rule_error(&app, payload.rule_id),
            }
            let emitted = match event {
                AppEvent::FileProcessed(payload) => app.emit(FILE_PROCESSED_EVENT, payload),
                AppEvent::RuleError(payload) => app.emit(RULE_ERROR_EVENT, payload),
                AppEvent::RulePaused(payload) => app.emit(RULE_PAUSED_EVENT, payload),
            };
            if let Err(error) = emitted {
                tracing::error!(%error, "não foi possível emitir um evento para a interface");
            }
        }
    });
}

fn log_event(event: &AppEvent) {
    let payload = match serde_json::to_string(event) {
        Ok(payload) => payload,
        Err(error) => {
            tracing::error!(%error, "não foi possível serializar um evento do monitor");
            return;
        }
    };
    match event {
        AppEvent::FileProcessed(_) => tracing::info!(event = %payload, "arquivo processado"),
        AppEvent::RuleError(_) => tracing::error!(event = %payload, "erro em regra"),
        AppEvent::RulePaused(_) => tracing::warn!(event = %payload, "regra pausada"),
    }
}

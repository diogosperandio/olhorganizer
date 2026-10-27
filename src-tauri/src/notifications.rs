use std::mem;
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;

use crate::events::{AppEvent, OutcomePayload};
use crate::state::AppState;

const BATCH_WINDOW: Duration = Duration::from_secs(10);
const GROUP_THRESHOLD: usize = 5;

#[derive(Default)]
struct Batch {
    messages: Vec<String>,
    scheduled: bool,
}

#[derive(Default)]
pub struct NotificationBatcher {
    batch: Mutex<Batch>,
}

impl NotificationBatcher {
    pub fn queue(&self, app: &AppHandle, message: String) {
        let should_schedule = match self.batch.lock() {
            Ok(mut batch) => {
                batch.messages.push(message);
                if batch.scheduled {
                    false
                } else {
                    batch.scheduled = true;
                    true
                }
            }
            Err(error) => {
                tracing::error!(%error, "não foi possível agrupar uma notificação");
                false
            }
        };
        if should_schedule {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(BATCH_WINDOW).await;
                flush(&app);
            });
        }
    }
}

pub async fn handle_monitor_event(app: &AppHandle, event: &AppEvent) {
    match event {
        AppEvent::FileProcessed(payload) => match &payload.outcome {
            OutcomePayload::Moved { to } => {
                if notifications_enabled(app).await {
                    queue_processed(app, format_message(&payload.file, "movido", to));
                }
            }
            OutcomePayload::Copied { to } => {
                if notifications_enabled(app).await {
                    queue_processed(app, format_message(&payload.file, "copiado", to));
                }
            }
            OutcomePayload::Failed { error } => send(app, "Erro ao organizar arquivo", error),
            _ => {}
        },
        AppEvent::RuleError(payload) => send(app, "Erro em uma regra", &payload.message_pt),
        AppEvent::RulePaused(payload) => send(app, "Regra pausada", &payload.reason_pt),
    }
}

pub fn send_background_hint(app: &AppHandle) {
    send(
        app,
        "Olhorganizer",
        "O Olhorganizer continua funcionando aqui.",
    );
}

fn queue_processed(app: &AppHandle, message: String) {
    app.state::<NotificationBatcher>().queue(app, message);
}

fn flush(app: &AppHandle) {
    let messages = match app.state::<NotificationBatcher>().batch.lock() {
        Ok(mut batch) => {
            batch.scheduled = false;
            mem::take(&mut batch.messages)
        }
        Err(error) => {
            tracing::error!(%error, "não foi possível concluir o agrupamento de notificações");
            return;
        }
    };
    if messages.len() > GROUP_THRESHOLD {
        send(
            app,
            "Olhorganizer",
            &format!("{} arquivos organizados", messages.len()),
        );
    } else {
        for message in messages {
            send(app, "Arquivo organizado", &message);
        }
    }
}

async fn notifications_enabled(app: &AppHandle) -> bool {
    app.state::<AppState>()
        .config
        .lock()
        .await
        .settings
        .notifications
}

fn format_message(file: &str, verb: &str, destination: &str) -> String {
    let name = Path::new(file)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(file);
    let folder = Path::new(destination)
        .parent()
        .map_or_else(|| destination.to_owned(), |path| path.display().to_string());
    format!("{name} {verb} para {folder}")
}

fn send(app: &AppHandle, title: &str, body: &str) {
    if let Err(error) = app.notification().builder().title(title).body(body).show() {
        tracing::warn!(%error, "não foi possível mostrar uma notificação");
    }
}

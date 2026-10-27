use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use motor::{Config, Uuid};
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{App, AppHandle, Emitter, Manager, Window, WindowEvent, Wry};

use crate::notifications;
use crate::state::AppState;

const TRAY_ID: &str = "olhorganizer-tray";
const OPEN_ID: &str = "open";
const PAUSE_ID: &str = "pause-all";
const HISTORY_ID: &str = "history";
const QUIT_ID: &str = "quit";
pub const NAVIGATE_HISTORY_EVENT: &str = "navigate-history";

#[derive(Clone, Copy)]
enum IconStatus {
    Normal,
    Paused,
    Error,
}

pub struct TrayState {
    available: AtomicBool,
    first_close: AtomicBool,
    all_disabled: AtomicBool,
    error_rules: Mutex<HashSet<Uuid>>,
    pause_item: Mutex<Option<MenuItem<Wry>>>,
}

impl TrayState {
    fn new(all_disabled: bool) -> Self {
        Self {
            available: AtomicBool::new(false),
            first_close: AtomicBool::new(true),
            all_disabled: AtomicBool::new(all_disabled),
            error_rules: Mutex::new(HashSet::new()),
            pause_item: Mutex::new(None),
        }
    }
}

pub fn setup(app: &mut App, config: &Config) {
    let all_disabled = config.rules.iter().all(|rule| !rule.enabled);
    app.manage(TrayState::new(all_disabled));

    match build(app, all_disabled) {
        Ok(pause_item) => {
            let state = app.state::<TrayState>();
            let can_hide = tray_can_restore_window();
            state.available.store(can_hide, Ordering::Relaxed);
            if let Ok(mut item) = state.pause_item.lock() {
                *item = Some(pause_item);
            };
            if !can_hide {
                tracing::info!(
                    "o GNOME sem suporte a indicadores foi detectado; a janela continuará minimizável e fechar encerrará o aplicativo"
                );
            }
        }
        Err(error) => tracing::warn!(
            %error,
            "a bandeja do sistema não está disponível; fechar a janela encerrará o aplicativo"
        ),
    }
}

fn build(app: &App, all_disabled: bool) -> tauri::Result<MenuItem<Wry>> {
    let open = MenuItem::with_id(app, OPEN_ID, "Abrir Olhorganizer", true, None::<&str>)?;
    let pause = MenuItem::with_id(app, PAUSE_ID, "Pausar todas as regras", true, None::<&str>)?;
    let history = MenuItem::with_id(app, HISTORY_ID, "Ver histórico", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, QUIT_ID, "Sair", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &pause, &history, &separator, &quit])?;
    let initial = if all_disabled {
        IconStatus::Paused
    } else {
        IconStatus::Normal
    };

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon(initial))
        .tooltip("Olhorganizer")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            OPEN_ID => show_main_window(app),
            PAUSE_ID => toggle_all(app.clone()),
            HISTORY_ID => {
                show_main_window(app);
                if let Err(error) = app.emit(NAVIGATE_HISTORY_EVENT, ()) {
                    tracing::warn!(%error, "não foi possível abrir o histórico pela bandeja");
                }
            }
            QUIT_ID => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(pause)
}

pub fn handle_window_event(window: &Window, event: &WindowEvent) {
    let WindowEvent::CloseRequested { api, .. } = event else {
        return;
    };
    let app = window.app_handle();
    let tray = app.state::<TrayState>();
    if !tray.available.load(Ordering::Relaxed) {
        return;
    }
    api.prevent_close();
    if let Err(error) = window.hide() {
        tracing::warn!(%error, "não foi possível ocultar a janela");
        return;
    }
    if tray.first_close.swap(false, Ordering::Relaxed) {
        notifications::send_background_hint(app);
    }
}

pub fn show_main_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        tracing::warn!("a janela principal não foi encontrada");
        return;
    };
    if let Err(error) = window.show() {
        tracing::warn!(%error, "não foi possível mostrar a janela principal");
    }
    if let Err(error) = window.unminimize() {
        tracing::warn!(%error, "não foi possível restaurar a janela principal");
    }
    if let Err(error) = window.set_focus() {
        tracing::warn!(%error, "não foi possível focar a janela principal");
    }
}

pub fn hide_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let tray_available = app
            .try_state::<TrayState>()
            .is_some_and(|state| state.available.load(Ordering::Relaxed));
        let result = if tray_available {
            window.hide()
        } else {
            window.minimize()
        };
        if let Err(error) = result {
            tracing::warn!(%error, "não foi possível iniciar minimizado");
        }
    }
}

#[cfg(target_os = "linux")]
fn tray_can_restore_window() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP")
        .map(|desktop| !desktop.eq_ignore_ascii_case("GNOME"))
        .unwrap_or(true)
}

#[cfg(not(target_os = "linux"))]
fn tray_can_restore_window() -> bool {
    true
}

pub fn sync_rules(app: &AppHandle, config: &Config) {
    let tray = app.state::<TrayState>();
    if let Ok(mut error_rules) = tray.error_rules.lock() {
        error_rules.retain(|id| {
            config
                .rules
                .iter()
                .any(|rule| rule.id == *id && rule.enabled)
        });
    }
    let paused_by_menu = app
        .state::<AppState>()
        .all_rules_paused
        .load(Ordering::Relaxed);
    tray.all_disabled.store(
        paused_by_menu || config.rules.iter().all(|rule| !rule.enabled),
        Ordering::Relaxed,
    );
    refresh_icon(app);
}

pub fn mark_rule_error(app: &AppHandle, rule_id: Uuid) {
    let tray = app.state::<TrayState>();
    if let Ok(mut rules) = tray.error_rules.lock() {
        rules.insert(rule_id);
    }
    refresh_icon(app);
}

pub fn clear_rule_error(app: &AppHandle, rule_id: Uuid) {
    let tray = app.state::<TrayState>();
    if let Ok(mut rules) = tray.error_rules.lock() {
        rules.remove(&rule_id);
    }
    refresh_icon(app);
}

fn toggle_all(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let _mutation = state.mutation.lock().await;
        let should_resume = state.all_rules_paused.load(Ordering::Relaxed);
        state
            .all_rules_paused
            .store(!should_resume, Ordering::Relaxed);

        let config = state.config.lock().await.clone();
        let monitor_config = if should_resume {
            config.clone()
        } else {
            disabled_config(&config)
        };
        state.monitor.lock().await.reload(monitor_config).await;

        let tray = app.state::<TrayState>();
        tray.all_disabled.store(
            !should_resume || config.rules.iter().all(|rule| !rule.enabled),
            Ordering::Relaxed,
        );
        set_pause_text(
            &tray,
            if should_resume {
                "Pausar todas as regras"
            } else {
                "Retomar todas"
            },
        );
        refresh_icon(&app);
    });
}

pub fn disabled_config(config: &Config) -> Config {
    let mut disabled = config.clone();
    for rule in &mut disabled.rules {
        rule.enabled = false;
    }
    disabled
}

fn set_pause_text(state: &TrayState, text: &str) {
    let item = state
        .pause_item
        .lock()
        .ok()
        .and_then(|item| item.as_ref().cloned());
    if let Some(item) = item {
        if let Err(error) = item.set_text(text) {
            tracing::warn!(%error, "não foi possível atualizar o menu da bandeja");
        }
    }
}

fn refresh_icon(app: &AppHandle) {
    let state = app.state::<TrayState>();
    if !state.available.load(Ordering::Relaxed) {
        return;
    }
    let has_error = state
        .error_rules
        .lock()
        .map(|rules| !rules.is_empty())
        .unwrap_or(true);
    let status = if has_error {
        IconStatus::Error
    } else if state.all_disabled.load(Ordering::Relaxed) {
        IconStatus::Paused
    } else {
        IconStatus::Normal
    };
    if let Some(tray) = app.tray_by_id(TRAY_ID) {
        if let Err(error) = tray.set_icon(Some(icon(status))) {
            tracing::warn!(%error, "não foi possível atualizar o ícone da bandeja");
        }
    }
}

fn icon(status: IconStatus) -> Image<'static> {
    let color = match status {
        IconStatus::Normal => [38, 118, 83, 255],
        IconStatus::Paused => [210, 154, 37, 255],
        IconStatus::Error => [210, 75, 75, 255],
    };
    let mut pixels = Vec::with_capacity(32 * 32 * 4);
    for y in 0_i32..32 {
        for x in 0_i32..32 {
            let distance = (x - 16).pow(2) + (y - 16).pow(2);
            let pixel = if distance <= 14_i32.pow(2) && distance >= 7_i32.pow(2) {
                color
            } else {
                [0, 0, 0, 0]
            };
            pixels.extend_from_slice(&pixel);
        }
    }
    Image::new_owned(pixels, 32, 32)
}

mod scan;
mod stable;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use motor::{CheckMode, Config, Outcome, Rule, Uuid};
use notify::event::{ModifyKind, RenameMode};
use notify::{EventKind, RecursiveMode};
use notify_debouncer_full::{new_debouncer, DebounceEventResult};
use tokio::sync::{mpsc, watch, Mutex, RwLock};
use tokio::task::JoinHandle;

use crate::scan::{check_rule_paths, is_network_path, scan_top_level};
use crate::stable::{wait_until_stable, StableOptions};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MonitorEvent {
    FileProcessed {
        rule_id: Uuid,
        file: PathBuf,
        outcome: Outcome,
    },
    RuleError {
        rule_id: Uuid,
        message_pt: String,
    },
    RulePaused {
        rule_id: Uuid,
        reason_pt: String,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct MonitorOptions {
    pub dry_run: bool,
    pub debounce: Duration,
    pub stable_for: Duration,
    pub stable_attempts: usize,
    pub instant_rescan_interval: Duration,
    pub unavailable_retry: Duration,
    pub interval_minute: Duration,
}

impl Default for MonitorOptions {
    fn default() -> Self {
        Self {
            dry_run: false,
            debounce: Duration::from_secs(1),
            stable_for: Duration::from_secs(2),
            stable_attempts: 10,
            instant_rescan_interval: Duration::from_secs(10 * 60),
            unavailable_retry: Duration::from_secs(60),
            interval_minute: Duration::from_secs(60),
        }
    }
}

pub struct Monitor {
    event_tx: mpsc::UnboundedSender<MonitorEvent>,
    options: MonitorOptions,
    tasks: HashMap<Uuid, RuleHandle>,
    active_rules: Arc<RwLock<Vec<Rule>>>,
    in_flight: Arc<Mutex<HashSet<PathBuf>>>,
    network_hints: Arc<Mutex<HashSet<Uuid>>>,
}

impl Monitor {
    pub fn start(config: Config, event_tx: mpsc::UnboundedSender<MonitorEvent>) -> Self {
        Self::start_with_options(config, event_tx, MonitorOptions::default())
    }

    pub fn start_with_options(
        config: Config,
        event_tx: mpsc::UnboundedSender<MonitorEvent>,
        options: MonitorOptions,
    ) -> Self {
        let rules: Vec<_> = config
            .rules
            .into_iter()
            .filter(|rule| rule.enabled)
            .collect();
        let mut monitor = Self {
            event_tx,
            options,
            tasks: HashMap::new(),
            active_rules: Arc::new(RwLock::new(rules.clone())),
            in_flight: Arc::new(Mutex::new(HashSet::new())),
            network_hints: Arc::new(Mutex::new(HashSet::new())),
        };
        for rule in rules {
            monitor.spawn_rule(rule);
        }
        monitor
    }

    pub async fn reload(&mut self, config: Config) {
        let enabled_rules: Vec<_> = config
            .rules
            .into_iter()
            .filter(|rule| rule.enabled)
            .collect();
        let mut incoming: HashMap<_, _> = enabled_rules
            .iter()
            .cloned()
            .map(|rule| (rule.id, rule))
            .collect();
        let current_ids: Vec<_> = self.tasks.keys().copied().collect();

        for rule_id in current_ids {
            let unchanged = self
                .tasks
                .get(&rule_id)
                .zip(incoming.get(&rule_id))
                .is_some_and(|(handle, rule)| handle.rule == *rule);
            if unchanged {
                incoming.remove(&rule_id);
                continue;
            }
            if let Some(handle) = self.tasks.remove(&rule_id) {
                handle.stop().await;
            }
        }

        *self.active_rules.write().await = enabled_rules;
        for rule in incoming.into_values() {
            self.spawn_rule(rule);
        }
    }

    pub async fn stop(&mut self) {
        let handles: Vec<_> = self.tasks.drain().map(|(_, handle)| handle).collect();
        for handle in handles {
            handle.stop().await;
        }
    }

    fn spawn_rule(&mut self, rule: Rule) {
        let (stop_tx, stop_rx) = watch::channel(false);
        let task_rule = rule.clone();
        let event_tx = self.event_tx.clone();
        let options = self.options;
        let active_rules = Arc::clone(&self.active_rules);
        let in_flight = Arc::clone(&self.in_flight);
        let network_hints = Arc::clone(&self.network_hints);
        let join = tokio::spawn(async move {
            run_rule(
                task_rule,
                event_tx,
                options,
                active_rules,
                in_flight,
                network_hints,
                stop_rx,
            )
            .await;
        });
        self.tasks.insert(
            rule.id,
            RuleHandle {
                rule,
                stop_tx,
                join,
            },
        );
    }
}

impl Drop for Monitor {
    fn drop(&mut self) {
        for handle in self.tasks.values() {
            let _ = handle.stop_tx.send(true);
        }
    }
}

struct RuleHandle {
    rule: Rule,
    stop_tx: watch::Sender<bool>,
    join: JoinHandle<()>,
}

impl RuleHandle {
    async fn stop(self) {
        let _ = self.stop_tx.send(true);
        if let Err(error) = self.join.await {
            tracing::warn!(%error, "a tarefa da regra terminou inesperadamente");
        }
    }
}

async fn run_rule(
    rule: Rule,
    event_tx: mpsc::UnboundedSender<MonitorEvent>,
    options: MonitorOptions,
    active_rules: Arc<RwLock<Vec<Rule>>>,
    in_flight: Arc<Mutex<HashSet<PathBuf>>>,
    network_hints: Arc<Mutex<HashSet<Uuid>>>,
    mut stop_rx: watch::Receiver<bool>,
) {
    emit_network_hint_once(&rule, &event_tx, &network_hints).await;
    let mut pause_announced = false;

    loop {
        if *stop_rx.borrow() {
            return;
        }
        if let Err(reason) = check_rule_paths(&rule) {
            if !pause_announced {
                send_event(
                    &event_tx,
                    MonitorEvent::RulePaused {
                        rule_id: rule.id,
                        reason_pt: reason,
                    },
                );
                pause_announced = true;
            }
            if wait_or_stop(options.unavailable_retry, &mut stop_rx).await {
                return;
            }
            continue;
        }
        pause_announced = false;

        let result = match rule.check_mode {
            CheckMode::Instant => {
                run_instant(
                    &rule,
                    &event_tx,
                    options,
                    &active_rules,
                    &in_flight,
                    &mut stop_rx,
                )
                .await
            }
            CheckMode::Interval { minutes } => {
                run_interval(
                    &rule,
                    &event_tx,
                    options,
                    minutes,
                    &active_rules,
                    &in_flight,
                    &mut stop_rx,
                )
                .await
            }
        };

        match result {
            SessionEnd::Stopped => return,
            SessionEnd::Unavailable(reason) => {
                send_event(
                    &event_tx,
                    MonitorEvent::RulePaused {
                        rule_id: rule.id,
                        reason_pt: reason,
                    },
                );
                pause_announced = true;
            }
            SessionEnd::Error(message) => {
                send_event(
                    &event_tx,
                    MonitorEvent::RuleError {
                        rule_id: rule.id,
                        message_pt: message,
                    },
                );
            }
        }

        if wait_or_stop(options.unavailable_retry, &mut stop_rx).await {
            return;
        }
    }
}

enum SessionEnd {
    Stopped,
    Unavailable(String),
    Error(String),
}

async fn run_instant(
    rule: &Rule,
    event_tx: &mpsc::UnboundedSender<MonitorEvent>,
    options: MonitorOptions,
    active_rules: &Arc<RwLock<Vec<Rule>>>,
    in_flight: &Arc<Mutex<HashSet<PathBuf>>>,
    stop_rx: &mut watch::Receiver<bool>,
) -> SessionEnd {
    let (notify_tx, mut notify_rx) = mpsc::unbounded_channel::<DebounceEventResult>();
    let mut debouncer = match new_debouncer(options.debounce, None, move |result| {
        let _ = notify_tx.send(result);
    }) {
        Ok(debouncer) => debouncer,
        Err(error) => {
            return SessionEnd::Error(format!(
                "Não foi possível iniciar o monitoramento da pasta: {error}"
            ));
        }
    };
    if let Err(error) = debouncer.watch(&rule.source_dir, RecursiveMode::NonRecursive) {
        return SessionEnd::Unavailable(format!(
            "Não foi possível observar a pasta '{}': {error}",
            rule.source_dir.display()
        ));
    }

    if let Some(end) = scan_and_process(rule, event_tx, options, active_rules, in_flight).await {
        return end;
    }

    let mut rescan = tokio::time::interval(options.instant_rescan_interval);
    rescan.tick().await;
    let mut health = tokio::time::interval(options.unavailable_retry);
    health.tick().await;

    loop {
        tokio::select! {
            changed = stop_rx.changed() => {
                if changed.is_err() || *stop_rx.borrow() {
                    return SessionEnd::Stopped;
                }
            }
            result = notify_rx.recv() => {
                let Some(result) = result else {
                    return SessionEnd::Error("O monitor de arquivos foi encerrado inesperadamente.".to_owned());
                };
                match result {
                    Ok(events) => {
                        for path in event_paths(events) {
                            if let Err(reason) = check_rule_paths(rule) {
                                return SessionEnd::Unavailable(reason);
                            }
                            if let Some(reason) = process_candidate(rule, path, event_tx, options, active_rules, in_flight).await {
                                return SessionEnd::Unavailable(reason);
                            }
                        }
                    }
                    Err(errors) => {
                        let message = errors.into_iter().map(|error| error.to_string()).collect::<Vec<_>>().join("; ");
                        send_event(event_tx, MonitorEvent::RuleError {
                            rule_id: rule.id,
                            message_pt: format!("O monitor da pasta informou um erro: {message}"),
                        });
                    }
                }
            }
            _ = rescan.tick() => {
                if let Some(end) = scan_and_process(rule, event_tx, options, active_rules, in_flight).await {
                    return end;
                }
            }
            _ = health.tick() => {
                if let Err(reason) = check_rule_paths(rule) {
                    return SessionEnd::Unavailable(reason);
                }
            }
        }
    }
}

async fn run_interval(
    rule: &Rule,
    event_tx: &mpsc::UnboundedSender<MonitorEvent>,
    options: MonitorOptions,
    minutes: u32,
    active_rules: &Arc<RwLock<Vec<Rule>>>,
    in_flight: &Arc<Mutex<HashSet<PathBuf>>>,
    stop_rx: &mut watch::Receiver<bool>,
) -> SessionEnd {
    if let Some(end) = scan_and_process(rule, event_tx, options, active_rules, in_flight).await {
        return end;
    }

    let period = options
        .interval_minute
        .checked_mul(minutes.max(1))
        .unwrap_or(Duration::MAX);
    let mut scans = tokio::time::interval(period);
    scans.tick().await;
    let mut health = tokio::time::interval(options.unavailable_retry);
    health.tick().await;

    loop {
        tokio::select! {
            changed = stop_rx.changed() => {
                if changed.is_err() || *stop_rx.borrow() {
                    return SessionEnd::Stopped;
                }
            }
            _ = scans.tick() => {
                if let Some(end) = scan_and_process(rule, event_tx, options, active_rules, in_flight).await {
                    return end;
                }
            }
            _ = health.tick() => {
                if let Err(reason) = check_rule_paths(rule) {
                    return SessionEnd::Unavailable(reason);
                }
            }
        }
    }
}

async fn scan_and_process(
    rule: &Rule,
    event_tx: &mpsc::UnboundedSender<MonitorEvent>,
    options: MonitorOptions,
    active_rules: &Arc<RwLock<Vec<Rule>>>,
    in_flight: &Arc<Mutex<HashSet<PathBuf>>>,
) -> Option<SessionEnd> {
    if let Err(reason) = check_rule_paths(rule) {
        return Some(SessionEnd::Unavailable(reason));
    }
    let files = match scan_top_level(&rule.source_dir) {
        Ok(files) => files,
        Err(error) => {
            return Some(SessionEnd::Unavailable(format!(
                "Não foi possível examinar a pasta '{}': {error}",
                rule.source_dir.display()
            )));
        }
    };

    for path in files {
        if let Some(reason) =
            process_candidate(rule, path, event_tx, options, active_rules, in_flight).await
        {
            return Some(SessionEnd::Unavailable(reason));
        }
    }
    None
}

async fn process_candidate(
    rule: &Rule,
    path: PathBuf,
    event_tx: &mpsc::UnboundedSender<MonitorEvent>,
    options: MonitorOptions,
    active_rules: &Arc<RwLock<Vec<Rule>>>,
    in_flight: &Arc<Mutex<HashSet<PathBuf>>>,
) -> Option<String> {
    let selected_rule_id = {
        let rules = active_rules.read().await;
        motor::pick_rule(&rules, &path).map(|selected| selected.id)
    };
    if selected_rule_id != Some(rule.id) {
        return None;
    }
    match std::fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.is_file() => {}
        Ok(_) => return None,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(_) => {}
    }

    let key = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
    {
        let mut paths = in_flight.lock().await;
        if !paths.insert(key.clone()) {
            return None;
        }
    }

    let stable_result = wait_until_stable(
        &path,
        StableOptions {
            stable_for: options.stable_for,
            attempts: options.stable_attempts,
        },
    )
    .await;
    if let Err(error) = stable_result {
        in_flight.lock().await.remove(&key);
        if let Err(reason) = check_rule_paths(rule) {
            return Some(reason);
        }
        send_event(
            event_tx,
            MonitorEvent::RuleError {
                rule_id: rule.id,
                message_pt: error.to_string(),
            },
        );
        return None;
    }

    if let Err(reason) = check_rule_paths(rule) {
        in_flight.lock().await.remove(&key);
        return Some(reason);
    }

    let task_rule = rule.clone();
    let task_path = path.clone();
    let dry_run = options.dry_run;
    let outcome = match tokio::task::spawn_blocking(move || {
        motor::ops::process(&task_path, &task_rule, dry_run)
    })
    .await
    {
        Ok(outcome) => outcome,
        Err(error) => Outcome::Failed {
            error: format!("A tarefa de processamento terminou inesperadamente: {error}"),
        },
    };

    in_flight.lock().await.remove(&key);
    send_event(
        event_tx,
        MonitorEvent::FileProcessed {
            rule_id: rule.id,
            file: path,
            outcome,
        },
    );
    None
}

fn event_paths(events: Vec<notify_debouncer_full::DebouncedEvent>) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for event in events {
        let relevant = matches!(
            event.kind,
            EventKind::Create(_)
                | EventKind::Modify(ModifyKind::Name(
                    RenameMode::To | RenameMode::Both | RenameMode::Any
                ))
        );
        if !relevant {
            continue;
        }
        if matches!(
            event.kind,
            EventKind::Modify(ModifyKind::Name(RenameMode::Both))
        ) {
            if let Some(path) = event.paths.last() {
                paths.push(path.clone());
            }
        } else {
            paths.extend(event.paths.iter().cloned());
        }
    }
    paths
}

async fn emit_network_hint_once(
    rule: &Rule,
    event_tx: &mpsc::UnboundedSender<MonitorEvent>,
    network_hints: &Arc<Mutex<HashSet<Uuid>>>,
) {
    if !matches!(rule.check_mode, CheckMode::Instant) || !is_network_path(&rule.source_dir) {
        return;
    }
    if network_hints.lock().await.insert(rule.id) {
        send_event(
            event_tx,
            MonitorEvent::RuleError {
                rule_id: rule.id,
                message_pt: "Dica: esta pasta parece estar em uma rede; o modo Intervalo costuma ser mais confiável para esse tipo de caminho.".to_owned(),
            },
        );
    }
}

async fn wait_or_stop(duration: Duration, stop_rx: &mut watch::Receiver<bool>) -> bool {
    tokio::select! {
        _ = tokio::time::sleep(duration) => false,
        changed = stop_rx.changed() => changed.is_err() || *stop_rx.borrow(),
    }
}

fn send_event(event_tx: &mpsc::UnboundedSender<MonitorEvent>, event: MonitorEvent) {
    if event_tx.send(event).is_err() {
        tracing::debug!("o receptor de eventos do monitor foi encerrado");
    }
}

//! Olhorganizer folder monitor: watches the source folder of every enabled
//! rule and hands new files to `motor::ops::process`. Has no Tauri dependency.

pub mod scan;
pub mod stable;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError, RwLock};
use std::time::Duration;

use motor::ops::{Outcome, process};
use motor::{CheckMode, Config, Rule, Uuid, compile, is_temporary, pick_rule};
use notify::RecursiveMode;
use notify::event::{EventKind, ModifyKind, RenameMode};
use notify_debouncer_full::{DebounceEventResult, new_debouncer};
use tokio::runtime::Handle;
use tokio::sync::mpsc::{self, UnboundedSender};
use tokio::task::{JoinHandle, JoinSet};
use tokio::time::{Instant, interval_at};

use crate::stable::{StableConfig, Unstable, wait_until_stable};

/// Something that happened while monitoring. Messages are in pt-BR.
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
    /// The source folder or destination disk is missing; retried periodically.
    RulePaused {
        rule_id: Uuid,
        reason_pt: String,
    },
    /// A paused rule found its folders again.
    RuleResumed {
        rule_id: Uuid,
    },
    /// Sent once per rule: instant mode on a network folder may miss files.
    NetworkHint {
        rule_id: Uuid,
        message_pt: String,
    },
}

impl MonitorEvent {
    pub fn rule_id(&self) -> Uuid {
        match self {
            Self::FileProcessed { rule_id, .. }
            | Self::RuleError { rule_id, .. }
            | Self::RulePaused { rule_id, .. }
            | Self::RuleResumed { rule_id }
            | Self::NetworkHint { rule_id, .. } => *rule_id,
        }
    }

    /// One pt-BR line, e.g. "20261022.pdf movido para /home/ana/Jornais/2026/10".
    pub fn describe_pt(&self) -> String {
        self.describe(None)
    }

    /// Like [`Self::describe_pt`], naming files in subfolders by their path
    /// relative to `source_dir`: "2025/fotos/IMG_0001.jpg copiado para ...".
    pub fn describe_pt_from(&self, source_dir: &Path) -> String {
        self.describe(Some(source_dir))
    }

    fn describe(&self, source_dir: Option<&Path>) -> String {
        match self {
            Self::FileProcessed { file, outcome, .. } => {
                describe_outcome(&file_label(file, source_dir), outcome)
            }
            Self::RuleError { message_pt, .. } | Self::NetworkHint { message_pt, .. } => {
                message_pt.clone()
            }
            Self::RulePaused { reason_pt, .. } => format!("Regra pausada. {reason_pt}"),
            Self::RuleResumed { .. } => "Regra retomada.".to_owned(),
        }
    }
}

/// The file name, or its `/`-separated path relative to `source_dir`.
fn file_label(file: &Path, source_dir: Option<&Path>) -> String {
    let relative = source_dir
        .and_then(|dir| file.strip_prefix(dir).ok())
        .filter(|rel| rel.components().count() > 0);
    match relative {
        Some(rel) => rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/"),
        None => file
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
    }
}

fn describe_outcome(name: &str, outcome: &Outcome) -> String {
    let file_name = name.rsplit('/').next().unwrap_or(name);
    let place = |to: &Path| {
        let dir = to.parent().unwrap_or(to).display();
        match to.file_name() {
            Some(new_name) if new_name.to_string_lossy() != file_name => {
                format!("{dir} como {}", new_name.to_string_lossy())
            }
            _ => dir.to_string(),
        }
    };
    match outcome {
        Outcome::Moved { to } => format!("{name} movido para {}", place(to)),
        Outcome::Copied { to } => format!("{name} copiado para {}", place(to)),
        Outcome::WouldMove { to } => format!("{name} seria movido para {} (simulação)", place(to)),
        Outcome::WouldCopy { to } => {
            format!("{name} seria copiado para {} (simulação)", place(to))
        }
        Outcome::Skipped { reason } => format!("{name} ignorado: {reason}"),
        Outcome::NotMatched => format!("{name} não corresponde ao filtro"),
        Outcome::Failed { error } => format!("Erro ao processar {name}: {error}"),
    }
}

#[derive(Debug, Clone)]
pub struct MonitorOptions {
    /// Report what would happen without touching any file.
    pub dry_run: bool,
    /// Quiet time before file system events are delivered.
    pub debounce: Duration,
    pub stable: StableConfig,
    /// Full rescan for instant rules, to catch missed events.
    pub rescan_every: Duration,
    /// How often a paused rule checks whether its folders are back.
    pub retry_unavailable: Duration,
    /// Length of one minute in `CheckMode::Interval`; shortened only in tests.
    pub minute: Duration,
}

impl Default for MonitorOptions {
    fn default() -> Self {
        Self {
            dry_run: false,
            debounce: Duration::from_secs(1),
            stable: StableConfig::default(),
            rescan_every: Duration::from_secs(10 * 60),
            retry_unavailable: Duration::from_secs(60),
            minute: Duration::from_secs(60),
        }
    }
}

/// State shared by every rule task.
#[derive(Clone)]
struct Shared {
    options: MonitorOptions,
    events: UnboundedSender<MonitorEvent>,
    /// The running rules, used to pick which rule owns a file.
    rules: Arc<RwLock<Vec<Rule>>>,
    /// Files being processed right now, by any rule.
    in_flight: Arc<Mutex<HashSet<PathBuf>>>,
}

impl Shared {
    fn emit(&self, event: MonitorEvent) {
        // The receiver is gone only while shutting down.
        let _ = self.events.send(event);
    }
}

/// Watches the enabled rules of a config. Dropping it stops everything.
pub struct Monitor {
    shared: Shared,
    runtime: Handle,
    tasks: HashMap<Uuid, (Rule, JoinHandle<()>)>,
    hinted: HashSet<Uuid>,
}

impl Monitor {
    /// Starts monitoring with default options. Must be called inside a Tokio runtime.
    pub fn start(config: &Config, event_tx: UnboundedSender<MonitorEvent>) -> Self {
        Self::start_with(config, event_tx, MonitorOptions::default())
    }

    pub fn start_with(
        config: &Config,
        event_tx: UnboundedSender<MonitorEvent>,
        options: MonitorOptions,
    ) -> Self {
        let mut monitor = Self {
            shared: Shared {
                options,
                events: event_tx,
                rules: Arc::default(),
                in_flight: Arc::default(),
            },
            runtime: Handle::current(),
            tasks: HashMap::new(),
            hinted: HashSet::new(),
        };
        monitor.reload(config);
        monitor
    }

    /// Applies a new config, restarting only the rules that changed.
    pub fn reload(&mut self, config: &Config) {
        let rules = self.runnable_rules(config);
        *self
            .shared
            .rules
            .write()
            .unwrap_or_else(PoisonError::into_inner) = rules.clone();

        let wanted: HashMap<Uuid, &Rule> = rules.iter().map(|r| (r.id, r)).collect();
        self.tasks.retain(|id, (running, handle)| {
            let keep = wanted.get(id).is_some_and(|rule| *rule == running);
            if !keep {
                handle.abort();
            }
            keep
        });

        for rule in rules {
            if self.tasks.contains_key(&rule.id) {
                continue;
            }
            self.network_hint(&rule);
            let handle = self
                .runtime
                .spawn(run_rule(rule.clone(), self.shared.clone()));
            self.tasks.insert(rule.id, (rule, handle));
        }
    }

    /// Stops every watcher. A file already being moved finishes first.
    pub fn stop(&mut self) {
        for (_, (_, handle)) in self.tasks.drain() {
            handle.abort();
        }
    }

    /// Enabled rules that pass validation; the others get a `RuleError`.
    fn runnable_rules(&self, config: &Config) -> Vec<Rule> {
        let mut rules = Vec::new();
        for rule in config.rules.iter().filter(|r| r.enabled) {
            let mut problems: Vec<String> =
                rule.validate().iter().map(ToString::to_string).collect();
            if let Err(e) = compile(&rule.filter) {
                problems.push(e.to_string());
            }
            if problems.is_empty() {
                rules.push(rule.clone());
            } else {
                self.shared.emit(MonitorEvent::RuleError {
                    rule_id: rule.id,
                    message_pt: format!(
                        "A regra “{}” não foi iniciada: {}",
                        rule.name,
                        problems.join(" ")
                    ),
                });
            }
        }
        rules
    }

    fn network_hint(&mut self, rule: &Rule) {
        if rule.check_mode != CheckMode::Instant
            || self.hinted.contains(&rule.id)
            || !scan::is_network_path(&rule.source_dir)
        {
            return;
        }
        self.hinted.insert(rule.id);
        self.shared.emit(MonitorEvent::NetworkHint {
            rule_id: rule.id,
            message_pt: format!(
                "A pasta {} fica na rede. Em pastas de rede, o modo instantâneo pode deixar arquivos passarem; prefira verificar por intervalo.",
                rule.source_dir.display()
            ),
        });
    }
}

impl Drop for Monitor {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Runs one rule until aborted: watch or poll while its folders are
/// available, pause and retry while they are not.
async fn run_rule(rule: Rule, shared: Shared) {
    // Per-file tasks; dropped (and aborted) together with this task.
    let mut files = JoinSet::new();
    let mut paused = false;
    let mut limit_reported = false;
    loop {
        let reason = match available(&rule).await {
            Ok(()) => {
                if paused {
                    paused = false;
                    shared.emit(MonitorEvent::RuleResumed { rule_id: rule.id });
                }
                match rule.check_mode {
                    CheckMode::Instant => {
                        watch(&rule, &shared, &mut files, &mut limit_reported).await
                    }
                    CheckMode::Interval { minutes } => {
                        poll(&rule, &shared, &mut files, minutes).await
                    }
                }
            }
            Err(reason) => reason,
        };
        if !paused {
            paused = true;
            shared.emit(MonitorEvent::RulePaused {
                rule_id: rule.id,
                reason_pt: reason,
            });
        }
        tokio::time::sleep(shared.options.retry_unavailable).await;
    }
}

/// Instant mode. Returns the reason when the rule has to pause.
async fn watch(
    rule: &Rule,
    shared: &Shared,
    files: &mut JoinSet<()>,
    limit_reported: &mut bool,
) -> String {
    let mode = if rule.include_subfolders {
        RecursiveMode::Recursive
    } else {
        RecursiveMode::NonRecursive
    };
    let (tx, mut rx) = mpsc::unbounded_channel();
    let debouncer = new_debouncer(shared.options.debounce, None, move |result| {
        let _ = tx.send(result);
    })
    .and_then(|mut d| d.watch(&rule.source_dir, mode).map(|()| d));
    let _debouncer = match debouncer {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!(rule = %rule.name, error = %e, "watcher failed");
            if is_watch_limit(&e) {
                report_watch_limit(rule, shared, limit_reported);
            }
            return format!(
                "Não foi possível acompanhar a pasta {}: {e}",
                rule.source_dir.display()
            );
        }
    };

    // Startup scan: files that arrived before the watcher.
    if let Err(reason) = scan_tree(rule, shared, files, &rule.source_dir).await {
        return reason;
    }

    let options = &shared.options;
    let mut rescan = interval_at(Instant::now() + options.rescan_every, options.rescan_every);
    let mut health = interval_at(
        Instant::now() + options.retry_unavailable,
        options.retry_unavailable,
    );
    loop {
        tokio::select! {
            Some(result) = rx.recv() => {
                let (paths, limit_hit) = arrived_paths(result, rule);
                if limit_hit {
                    report_watch_limit(rule, shared, limit_reported);
                }
                for path in paths {
                    let is_dir = std::fs::symlink_metadata(&path).is_ok_and(|m| m.is_dir());
                    if !is_dir {
                        dispatch(rule, shared, files, path);
                    } else if rule.include_subfolders && rule.watches(&path.join("_")) {
                        // A folder moved or copied in may bring files that
                        // never produce events of their own.
                        if let Err(reason) = scan_tree(rule, shared, files, &path).await {
                            tracing::warn!(rule = %rule.name, %reason, "new folder not scanned");
                        }
                    }
                }
                if let Err(reason) = available(rule).await {
                    return reason;
                }
            }
            _ = rescan.tick() => {
                if let Err(reason) = scan_tree(rule, shared, files, &rule.source_dir).await {
                    return reason;
                }
            }
            _ = health.tick() => {
                if let Err(reason) = available(rule).await {
                    return reason;
                }
            }
            Some(_) = files.join_next(), if !files.is_empty() => {}
        }
    }
}

/// Interval mode. Returns the reason when the rule has to pause.
async fn poll(rule: &Rule, shared: &Shared, files: &mut JoinSet<()>, minutes: u32) -> String {
    let period = shared.options.minute * minutes.max(1);
    // The first tick fires at once, which is the startup scan.
    let mut tick = tokio::time::interval(period);
    loop {
        tokio::select! {
            _ = tick.tick() => {
                if let Err(reason) = available(rule).await {
                    return reason;
                }
                if let Err(reason) = scan_tree(rule, shared, files, &rule.source_dir).await {
                    return reason;
                }
            }
            Some(_) = files.join_next(), if !files.is_empty() => {}
        }
    }
}

/// Whether adding a watch failed because the system limit was reached
/// (inotify's max_user_watches on Linux).
fn is_watch_limit(error: &notify::Error) -> bool {
    match &error.kind {
        notify::ErrorKind::MaxFilesWatch => true,
        // ENOSPC from inotify_add_watch means the watch limit, not a full disk.
        notify::ErrorKind::Io(e) => cfg!(target_os = "linux") && e.raw_os_error() == Some(28),
        _ => false,
    }
}

fn report_watch_limit(rule: &Rule, shared: &Shared, reported: &mut bool) {
    if std::mem::replace(reported, true) {
        return;
    }
    shared.emit(MonitorEvent::RuleError {
        rule_id: rule.id,
        message_pt: format!(
            "A pasta {} tem subpastas demais para o acompanhamento instantâneo; o sistema atingiu o limite de pastas acompanhadas. Mude a regra “{}” para verificar “A cada X minutos”.",
            rule.source_dir.display(),
            rule.name
        ),
    });
}

/// Paths a batch of debounced events brought into the folder (created
/// files and folders, and the new name of renamed ones), and whether the
/// batch reported the watch limit.
fn arrived_paths(result: DebounceEventResult, rule: &Rule) -> (Vec<PathBuf>, bool) {
    let events = match result {
        Ok(events) => events,
        Err(errors) => {
            for e in &errors {
                tracing::warn!(rule = %rule.name, error = %e, "watch error");
            }
            return (Vec::new(), errors.iter().any(is_watch_limit));
        }
    };
    let paths = events
        .iter()
        .filter_map(|event| match event.kind {
            EventKind::Create(_)
            | EventKind::Modify(ModifyKind::Name(
                RenameMode::To | RenameMode::Both | RenameMode::Any,
            )) => event.paths.last().cloned(),
            _ => None,
        })
        .collect();
    (paths, false)
}

async fn available(rule: &Rule) -> Result<(), String> {
    let rule = rule.clone();
    tokio::task::spawn_blocking(move || scan::check_available(&rule))
        .await
        .unwrap_or_else(|e| Err(format!("Falha interna ao verificar a regra: {e}")))
}

/// Dispatches the files in `root`, and below it if the rule includes subfolders.
async fn scan_tree(
    rule: &Rule,
    shared: &Shared,
    files: &mut JoinSet<()>,
    root: &Path,
) -> Result<(), String> {
    let dir = root.to_path_buf();
    let recursive = rule.include_subfolders;
    let listed = tokio::task::spawn_blocking(move || scan::list_files(&dir, recursive)).await;
    match listed {
        Ok(Ok(paths)) => {
            for path in paths {
                dispatch(rule, shared, files, path);
            }
            Ok(())
        }
        Ok(Err(e)) => Err(format!(
            "Não foi possível ler a pasta {}: {e}",
            root.display()
        )),
        Err(e) => Err(format!("Falha interna ao ler a pasta: {e}")),
    }
}

/// Starts processing `path` if this rule owns it and nobody else is on it.
fn dispatch(rule: &Rule, shared: &Shared, files: &mut JoinSet<()>, path: PathBuf) {
    let temporary = path
        .file_name()
        .is_none_or(|n| is_temporary(&n.to_string_lossy()));
    if temporary {
        return;
    }
    let owner = {
        let rules = shared.rules.read().unwrap_or_else(PoisonError::into_inner);
        pick_rule(&rules, &path).map(|r| r.id)
    };
    if owner != Some(rule.id) {
        return;
    }
    let Some(guard) = InFlight::claim(&shared.in_flight, &path) else {
        return;
    };

    let rule = rule.clone();
    let shared = shared.clone();
    files.spawn(async move {
        match wait_until_stable(&path, shared.options.stable).await {
            Ok(()) => {}
            Err(Unstable::Gone) => return,
            Err(Unstable::StillChanging) => {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                shared.emit(MonitorEvent::RuleError {
                    rule_id: rule.id,
                    message_pt: format!(
                        "{name} continua sendo alterado ou está em uso e não foi processado. Ele será verificado de novo mais tarde."
                    ),
                });
                return;
            }
        }

        let dry_run = shared.options.dry_run;
        let (file, rule_id) = (path.clone(), rule.id);
        // The guard moves into the blocking call: it outlives this task if
        // the task is aborted mid-move, so nobody else picks the file up.
        let outcome = tokio::task::spawn_blocking(move || {
            let _guard = guard;
            process(&file, &rule, dry_run)
        })
            .await
            .unwrap_or_else(|e| Outcome::Failed {
                error: format!("Falha interna ao processar o arquivo: {e}"),
            });
        if outcome != Outcome::NotMatched {
            shared.emit(MonitorEvent::FileProcessed {
                rule_id,
                file: path.clone(),
                outcome,
            });
        }
    });
}

/// Marks a path as being processed until dropped.
struct InFlight {
    set: Arc<Mutex<HashSet<PathBuf>>>,
    path: PathBuf,
}

impl InFlight {
    fn claim(set: &Arc<Mutex<HashSet<PathBuf>>>, path: &Path) -> Option<Self> {
        let inserted = set
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(path.to_path_buf());
        inserted.then(|| Self {
            set: Arc::clone(set),
            path: path.to_path_buf(),
        })
    }
}

impl Drop for InFlight {
    fn drop(&mut self) {
        self.set
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&self.path);
    }
}

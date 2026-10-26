//! Olhorganizer folder monitor: watches the source folder of every enabled
//! rule and hands new files to `motor::ops::process`. Has no Tauri dependency.

pub mod breaker;
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

use crate::breaker::{BREAKER_MESSAGE, Breaker, Identity, WriteLog};
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
    /// The source folder or destination disk is missing (retried
    /// periodically), or the circuit breaker stopped the rule (resumed only by
    /// [`Monitor::resume`] or a changed rule).
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
            Self::RulePaused { reason_pt, .. } => reason_pt.clone(),
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
        Outcome::AlreadyCopied { to } => format!("{name} já tinha sido copiado para {}", place(to)),
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
    /// Watcher events for a file the app itself wrote this recently are ignored.
    pub own_event_grace: Duration,
    /// Circuit breaker: a rule processing the same file more than
    /// `breaker_max_passes` times within `breaker_window` is paused.
    pub breaker_window: Duration,
    pub breaker_max_passes: u32,
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
            own_event_grace: Duration::from_secs(5),
            breaker_window: Duration::from_secs(10 * 60),
            breaker_max_passes: 3,
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
    /// Files the app moved or copied recently.
    writes: Arc<Mutex<WriteLog>>,
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
                writes: Arc::default(),
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

    /// Restarts a rule, e.g. after the circuit breaker paused it. Its pass
    /// counts start from zero.
    pub fn resume(&mut self, rule_id: Uuid) {
        if let Some((rule, handle)) = self.tasks.get_mut(&rule_id) {
            handle.abort();
            *handle = self
                .runtime
                .spawn(run_rule(rule.clone(), self.shared.clone()));
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

/// Why a rule's watch or poll loop ended.
enum Stop {
    /// Folders missing; retried periodically.
    Unavailable(String),
    /// The circuit breaker tripped; the rule stays paused.
    Tripped,
}

/// Per-rule state shared with its file tasks.
struct RuleCtx {
    rule: Rule,
    shared: Shared,
    breaker: Breaker,
}

/// Runs one rule until aborted or tripped: watch or poll while its folders
/// are available, pause and retry while they are not.
async fn run_rule(rule: Rule, shared: Shared) {
    let breaker = Breaker::new(
        shared.options.breaker_window,
        shared.options.breaker_max_passes,
    );
    let ctx = Arc::new(RuleCtx {
        rule,
        shared,
        breaker,
    });
    let (rule, shared) = (&ctx.rule, &ctx.shared);
    // Per-file tasks; dropped (and aborted) together with this task.
    let mut files = JoinSet::new();
    let mut paused = false;
    let mut limit_reported = false;
    loop {
        let stop = match available(rule).await {
            Ok(()) => {
                if paused {
                    paused = false;
                    shared.emit(MonitorEvent::RuleResumed { rule_id: rule.id });
                }
                match rule.check_mode {
                    CheckMode::Instant => watch(&ctx, &mut files, &mut limit_reported).await,
                    CheckMode::Interval { minutes } => poll(&ctx, &mut files, minutes).await,
                }
            }
            Err(reason) => Stop::Unavailable(reason),
        };
        match stop {
            Stop::Tripped => {
                tracing::warn!(rule = %rule.name, "circuit breaker tripped");
                shared.emit(MonitorEvent::RulePaused {
                    rule_id: rule.id,
                    reason_pt: BREAKER_MESSAGE.to_owned(),
                });
                return;
            }
            Stop::Unavailable(reason) => {
                if !paused {
                    paused = true;
                    shared.emit(MonitorEvent::RulePaused {
                        rule_id: rule.id,
                        reason_pt: format!("Regra pausada: {reason}"),
                    });
                }
            }
        }
        tokio::time::sleep(shared.options.retry_unavailable).await;
    }
}

/// Instant mode. Returns why the rule has to pause.
async fn watch(ctx: &Arc<RuleCtx>, files: &mut JoinSet<()>, limit_reported: &mut bool) -> Stop {
    let (rule, shared) = (&ctx.rule, &ctx.shared);
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
            return Stop::Unavailable(format!(
                "Não foi possível acompanhar a pasta {}: {e}",
                rule.source_dir.display()
            ));
        }
    };

    // Startup scan: files that arrived before the watcher.
    if let Err(reason) = scan_tree(ctx, files, &rule.source_dir).await {
        return Stop::Unavailable(reason);
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
                        dispatch(ctx, files, path, true);
                    } else if rule.include_subfolders && rule.watches(&path.join("_")) {
                        // A folder moved or copied in may bring files that
                        // never produce events of their own.
                        if let Err(reason) = scan_tree(ctx, files, &path).await {
                            tracing::warn!(rule = %rule.name, %reason, "new folder not scanned");
                        }
                    }
                }
                if let Err(reason) = available(rule).await {
                    return Stop::Unavailable(reason);
                }
            }
            _ = rescan.tick() => {
                if let Err(reason) = scan_tree(ctx, files, &rule.source_dir).await {
                    return Stop::Unavailable(reason);
                }
            }
            _ = health.tick() => {
                if let Err(reason) = available(rule).await {
                    return Stop::Unavailable(reason);
                }
            }
            () = ctx.breaker.tripped() => return Stop::Tripped,
            Some(_) = files.join_next(), if !files.is_empty() => {}
        }
    }
}

/// Interval mode. Returns why the rule has to pause.
async fn poll(ctx: &Arc<RuleCtx>, files: &mut JoinSet<()>, minutes: u32) -> Stop {
    let rule = &ctx.rule;
    let period = ctx.shared.options.minute * minutes.max(1);
    // The first tick fires at once, which is the startup scan.
    let mut tick = tokio::time::interval(period);
    loop {
        tokio::select! {
            _ = tick.tick() => {
                if let Err(reason) = available(rule).await {
                    return Stop::Unavailable(reason);
                }
                if let Err(reason) = scan_tree(ctx, files, &rule.source_dir).await {
                    return Stop::Unavailable(reason);
                }
            }
            () = ctx.breaker.tripped() => return Stop::Tripped,
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
async fn scan_tree(ctx: &Arc<RuleCtx>, files: &mut JoinSet<()>, root: &Path) -> Result<(), String> {
    let dir = root.to_path_buf();
    let recursive = ctx.rule.include_subfolders;
    let listed = tokio::task::spawn_blocking(move || scan::list_files(&dir, recursive)).await;
    match listed {
        Ok(Ok(paths)) => {
            for path in paths {
                dispatch(ctx, files, path, false);
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
/// `from_watcher` marks watcher events, which are ignored for files the app
/// itself just wrote.
fn dispatch(ctx: &Arc<RuleCtx>, files: &mut JoinSet<()>, path: PathBuf, from_watcher: bool) {
    let (rule, shared) = (&ctx.rule, &ctx.shared);
    let temporary = path
        .file_name()
        .is_none_or(|n| is_temporary(&n.to_string_lossy()));
    if temporary {
        return;
    }
    if from_watcher {
        let own = std::fs::metadata(&path).is_ok_and(|meta| {
            shared
                .writes
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .is_own_write(&path, &meta, shared.options.own_event_grace)
        });
        if own {
            return;
        }
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

    let ctx = Arc::clone(ctx);
    files.spawn(async move {
        let (rule, shared) = (&ctx.rule, &ctx.shared);
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
        // A dry run changes nothing, so it cannot loop.
        let identity = if dry_run {
            None
        } else {
            let Ok(meta) = std::fs::metadata(&path) else {
                return;
            };
            let identity = Identity::of(&path, &meta);
            if !ctx.breaker.allows(&identity) {
                return;
            }
            Some(identity)
        };

        let (file, task_ctx) = (path.clone(), Arc::clone(&ctx));
        // The guard moves into the blocking call: it outlives this task if
        // the task is aborted mid-move, so nobody else picks the file up.
        let outcome = tokio::task::spawn_blocking(move || {
            let _guard = guard;
            let outcome = process(&file, &task_ctx.rule, dry_run);
            if let Outcome::Moved { to } | Outcome::Copied { to } = &outcome {
                remember_write(&task_ctx.shared, to);
            }
            outcome
        })
        .await
        .unwrap_or_else(|e| Outcome::Failed {
            error: format!("Falha interna ao processar o arquivo: {e}"),
        });

        if let (Outcome::Moved { .. } | Outcome::Copied { .. }, Some(identity)) =
            (&outcome, identity)
        {
            ctx.breaker.record(identity);
        }
        if !matches!(outcome, Outcome::NotMatched | Outcome::AlreadyCopied { .. }) {
            shared.emit(MonitorEvent::FileProcessed {
                rule_id: rule.id,
                file: path.clone(),
                outcome,
            });
        }
    });
}

fn remember_write(shared: &Shared, path: &Path) {
    if let Ok(meta) = std::fs::metadata(path) {
        shared
            .writes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .record(path.to_path_buf(), &meta, shared.options.breaker_window);
    }
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

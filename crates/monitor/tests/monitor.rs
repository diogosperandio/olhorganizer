//! End-to-end tests: real folders, real watcher, shortened timings.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use monitor::stable::StableConfig;
use monitor::{Monitor, MonitorEvent, MonitorOptions};
use motor::ops::Outcome;
use motor::{Action, CheckMode, Config, ConflictPolicy, DateSource, Rule, Uuid};
use tokio::sync::mpsc::{self, UnboundedReceiver};
use tokio::time::timeout;

const WAIT: Duration = Duration::from_secs(10);

fn options() -> MonitorOptions {
    MonitorOptions {
        dry_run: false,
        debounce: Duration::from_millis(200),
        stable: StableConfig {
            quiet: Duration::from_millis(300),
            attempts: 10,
        },
        rescan_every: Duration::from_secs(3600),
        retry_unavailable: Duration::from_millis(300),
        minute: Duration::from_millis(300),
    }
}

struct Env {
    _dir: tempfile::TempDir,
    src: PathBuf,
    dst: PathBuf,
}

fn env() -> Env {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("NEWS");
    let dst = dir.path().join("Jornais");
    fs::create_dir_all(&src).unwrap();
    Env {
        src,
        dst,
        _dir: dir,
    }
}

fn rule(env: &Env, check_mode: CheckMode) -> Rule {
    Rule {
        id: Uuid::new_v4(),
        name: "Jornais".into(),
        enabled: true,
        source_dir: env.src.clone(),
        filter: "{YYYY}{MM}{DD}.pdf".into(),
        destination: format!("{}/{{YYYY}}/{{MM}}", env.dst.display()),
        action: Action::Move,
        date_source: DateSource::FileName,
        check_mode,
        on_conflict: ConflictPolicy::Rename,
        priority: 0,
    }
}

fn start(rule: Rule) -> (Monitor, UnboundedReceiver<MonitorEvent>) {
    let config = Config {
        rules: vec![rule],
        ..Config::default()
    };
    let (tx, rx) = mpsc::unbounded_channel();
    (Monitor::start_with(&config, tx, options()), rx)
}

async fn next_event(rx: &mut UnboundedReceiver<MonitorEvent>) -> MonitorEvent {
    timeout(WAIT, rx.recv())
        .await
        .expect("no event in time")
        .expect("channel closed")
}

/// Waits for the next `FileProcessed` and returns its outcome.
async fn next_outcome(rx: &mut UnboundedReceiver<MonitorEvent>) -> Outcome {
    loop {
        if let MonitorEvent::FileProcessed { outcome, .. } = next_event(rx).await {
            return outcome;
        }
    }
}

fn moved_to(env: &Env, name: &str) -> Outcome {
    Outcome::Moved {
        to: env.dst.join("2026").join("10").join(name),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn created_file_is_moved() {
    let env = env();
    let (_monitor, mut rx) = start(rule(&env, CheckMode::Instant));
    tokio::time::sleep(Duration::from_millis(300)).await;

    fs::write(env.src.join("20261022.pdf"), "jornal").unwrap();
    fs::write(env.src.join("foto.png"), "ignorada").unwrap();

    assert_eq!(next_outcome(&mut rx).await, moved_to(&env, "20261022.pdf"));
    assert_eq!(
        fs::read_to_string(env.dst.join("2026/10/20261022.pdf")).unwrap(),
        "jornal"
    );
    assert!(env.src.join("foto.png").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn slowly_written_file_waits_until_complete() {
    let env = env();
    let (_monitor, mut rx) = start(rule(&env, CheckMode::Instant));
    tokio::time::sleep(Duration::from_millis(300)).await;

    let path = env.src.join("20261022.pdf");
    let mut file = fs::File::create(&path).unwrap();
    for _ in 0..12 {
        file.write_all(&[b'x'; 1000]).unwrap();
        file.flush().unwrap();
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert!(
            rx.try_recv().is_err(),
            "processed while still being written"
        );
    }
    drop(file);

    assert_eq!(next_outcome(&mut rx).await, moved_to(&env, "20261022.pdf"));
    let moved = fs::metadata(env.dst.join("2026/10/20261022.pdf")).unwrap();
    assert_eq!(moved.len(), 12_000);
}

#[tokio::test(flavor = "multi_thread")]
async fn interval_mode_picks_up_new_files() {
    let env = env();
    let (_monitor, mut rx) = start(rule(&env, CheckMode::Interval { minutes: 1 }));
    tokio::time::sleep(Duration::from_millis(100)).await;

    fs::write(env.src.join("20261022.pdf"), "jornal").unwrap();
    assert_eq!(next_outcome(&mut rx).await, moved_to(&env, "20261022.pdf"));

    fs::write(env.src.join("20261023.pdf"), "outro").unwrap();
    assert_eq!(next_outcome(&mut rx).await, moved_to(&env, "20261023.pdf"));
}

#[tokio::test(flavor = "multi_thread")]
async fn startup_scan_processes_existing_files() {
    for mode in [CheckMode::Instant, CheckMode::Interval { minutes: 60 }] {
        let env = env();
        fs::write(env.src.join("20261022.pdf"), "jornal").unwrap();
        let (_monitor, mut rx) = start(rule(&env, mode));
        assert_eq!(
            next_outcome(&mut rx).await,
            moved_to(&env, "20261022.pdf"),
            "{mode:?}"
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn missing_folder_pauses_then_resumes() {
    let env = env();
    let r = rule(&env, CheckMode::Instant);
    fs::remove_dir(&env.src).unwrap();
    let (_monitor, mut rx) = start(r.clone());

    assert!(
        matches!(next_event(&mut rx).await, MonitorEvent::RulePaused { rule_id, .. } if rule_id == r.id)
    );

    fs::create_dir(&env.src).unwrap();
    fs::write(env.src.join("20261022.pdf"), "jornal").unwrap();
    assert_eq!(
        next_event(&mut rx).await,
        MonitorEvent::RuleResumed { rule_id: r.id }
    );
    assert_eq!(next_outcome(&mut rx).await, moved_to(&env, "20261022.pdf"));
}

#[tokio::test(flavor = "multi_thread")]
async fn invalid_rule_is_reported_and_not_started() {
    let env = env();
    let mut r = rule(&env, CheckMode::Instant);
    r.destination = format!("{}/{{YYYY}}", env.src.display());
    let (_monitor, mut rx) = start(r.clone());

    let event = next_event(&mut rx).await;
    assert!(
        matches!(&event, MonitorEvent::RuleError { message_pt, .. } if message_pt.contains("dentro da pasta monitorada")),
        "{event:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn dry_run_leaves_files_in_place() {
    let env = env();
    fs::write(env.src.join("20261022.pdf"), "jornal").unwrap();
    let config = Config {
        rules: vec![rule(&env, CheckMode::Instant)],
        ..Config::default()
    };
    let (tx, mut rx) = mpsc::unbounded_channel();
    let _monitor = Monitor::start_with(
        &config,
        tx,
        MonitorOptions {
            dry_run: true,
            ..options()
        },
    );

    let to = env.dst.join("2026/10/20261022.pdf");
    assert_eq!(next_outcome(&mut rx).await, Outcome::WouldMove { to });
    assert!(env.src.join("20261022.pdf").exists());
    assert!(!Path::new(&env.dst).exists());
}

#[test]
fn describes_events_in_portuguese() {
    let event = MonitorEvent::FileProcessed {
        rule_id: Uuid::nil(),
        file: PathBuf::from("/home/ana/Downloads/NEWS/20261022.pdf"),
        outcome: Outcome::Moved {
            to: PathBuf::from("/home/ana/Jornais/2026/10/20261022.pdf"),
        },
    };
    assert_eq!(
        event.describe_pt(),
        "20261022.pdf movido para /home/ana/Jornais/2026/10"
    );
}

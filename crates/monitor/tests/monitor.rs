use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use monitor::{Monitor, MonitorEvent, MonitorOptions};
use motor::{Action, CheckMode, Config, ConflictPolicy, DateSource, Outcome, Rule, Uuid};
use tempfile::tempdir;
use tokio::sync::mpsc;

fn rule(source: &Path, output: &Path, check_mode: CheckMode) -> Rule {
    Rule {
        id: Uuid::new_v4(),
        name: "Jornais".to_owned(),
        enabled: true,
        source_dir: source.to_path_buf(),
        filter: "{YYYY}{MM}{DD}.pdf".to_owned(),
        destination: output.join("{YYYY}/{MM}").to_string_lossy().into_owned(),
        action: Action::Move,
        date_source: DateSource::FileName,
        check_mode,
        include_subfolders: false,
        on_conflict: ConflictPolicy::Rename,
        priority: 0,
    }
}

fn config(rule: Rule) -> Config {
    Config {
        rules: vec![rule],
        ..Config::default()
    }
}

fn test_options() -> MonitorOptions {
    MonitorOptions {
        debounce: Duration::from_millis(30),
        stable_for: Duration::from_millis(80),
        stable_attempts: 10,
        instant_rescan_interval: Duration::from_secs(30),
        unavailable_retry: Duration::from_millis(100),
        interval_minute: Duration::from_millis(100),
        ..MonitorOptions::default()
    }
}

async fn next_processed(
    receiver: &mut mpsc::UnboundedReceiver<MonitorEvent>,
) -> (PathBuf, Outcome) {
    tokio::time::timeout(Duration::from_secs(4), async {
        match receiver.recv().await {
            Some(MonitorEvent::FileProcessed { file, outcome, .. }) => (file, outcome),
            Some(MonitorEvent::RuleError { message_pt, .. }) => {
                panic!("unexpected rule error: {message_pt}");
            }
            Some(MonitorEvent::RulePaused { reason_pt, .. }) => {
                panic!("unexpected paused rule: {reason_pt}");
            }
            None => panic!("event channel closed"),
        }
    })
    .await
    .expect("a processed event should arrive")
}

fn setup() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let directory = tempdir().expect("temp directory should be created");
    let source = directory.path().join("entrada");
    let output = directory.path().join("saida");
    fs::create_dir(&source).expect("source should be created");
    fs::create_dir(&output).expect("destination root should be created");
    (directory, source, output)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn created_file_is_moved() {
    let (_directory, source, output) = setup();
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let mut monitor = Monitor::start_with_options(
        config(rule(&source, &output, CheckMode::Instant)),
        event_tx,
        test_options(),
    );
    let file = source.join("20261022.pdf");
    fs::write(&file, b"jornal").expect("fixture should be written");

    let (_, outcome) = next_processed(&mut event_rx).await;
    let destination = output.join("2026/10/20261022.pdf");

    assert_eq!(
        outcome,
        Outcome::Moved {
            to: destination.clone()
        }
    );
    assert_eq!(fs::read(destination).expect("file should move"), b"jornal");
    assert!(!file.exists());
    monitor.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn slowly_written_file_moves_only_after_it_stops_growing() {
    let (_directory, source, output) = setup();
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let mut monitor = Monitor::start_with_options(
        config(rule(&source, &output, CheckMode::Instant)),
        event_tx,
        test_options(),
    );
    let file = source.join("20261022.pdf");
    fs::write(&file, b"parte-1").expect("first chunk should be written");
    let writer_path = file.clone();
    let writer = tokio::spawn(async move {
        for chunk in [b"-parte-2".as_slice(), b"-parte-3".as_slice()] {
            tokio::time::sleep(Duration::from_millis(60)).await;
            let mut writer = fs::OpenOptions::new()
                .append(true)
                .open(&writer_path)
                .expect("file should open for append");
            writer.write_all(chunk).expect("chunk should be written");
            writer.sync_all().expect("chunk should be synced");
        }
    });

    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(file.exists(), "file moved while it was still growing");
    assert!(!output.join("2026/10/20261022.pdf").exists());

    writer.await.expect("writer should finish");
    let (_, outcome) = next_processed(&mut event_rx).await;
    let destination = output.join("2026/10/20261022.pdf");

    assert!(matches!(outcome, Outcome::Moved { .. }));
    assert_eq!(
        fs::read(destination).expect("complete file should move"),
        b"parte-1-parte-2-parte-3"
    );
    monitor.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn interval_mode_scans_for_new_files() {
    let (_directory, source, output) = setup();
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let mut monitor = Monitor::start_with_options(
        config(rule(&source, &output, CheckMode::Interval { minutes: 1 })),
        event_tx,
        test_options(),
    );
    tokio::time::sleep(Duration::from_millis(40)).await;
    let file = source.join("20261022.pdf");
    fs::write(&file, b"intervalo").expect("fixture should be written");

    let (_, outcome) = next_processed(&mut event_rx).await;

    assert!(matches!(outcome, Outcome::Moved { .. }));
    assert_eq!(
        fs::read(output.join("2026/10/20261022.pdf")).expect("interval scan should move file"),
        b"intervalo"
    );
    monitor.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn startup_scan_processes_existing_files() {
    let (_directory, source, output) = setup();
    let file = source.join("20261022.pdf");
    fs::write(&file, "já existia".as_bytes()).expect("fixture should be written");
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let mut monitor = Monitor::start_with_options(
        config(rule(&source, &output, CheckMode::Instant)),
        event_tx,
        test_options(),
    );

    let (_, outcome) = next_processed(&mut event_rx).await;

    assert!(matches!(outcome, Outcome::Moved { .. }));
    assert_eq!(
        fs::read(output.join("2026/10/20261022.pdf")).expect("startup scan should move file"),
        "já existia".as_bytes()
    );
    monitor.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reload_keeps_unchanged_watcher_running_without_restarting_it() {
    let (_directory, source, output) = setup();
    fs::write(source.join("20261022.pdf"), b"permanece").expect("fixture should be written");
    let configuration = config(rule(&source, &output, CheckMode::Instant));
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let mut options = test_options();
    options.dry_run = true;
    let mut monitor = Monitor::start_with_options(configuration.clone(), event_tx, options);

    let (_, outcome) = next_processed(&mut event_rx).await;
    assert!(matches!(outcome, Outcome::WouldMove { .. }));

    monitor.reload(configuration).await;
    let duplicate = tokio::time::timeout(Duration::from_millis(250), event_rx.recv()).await;

    assert!(
        duplicate.is_err(),
        "unchanged rule was restarted during reload"
    );
    monitor.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reload_restarts_rule_when_include_subfolders_changes() {
    let (_directory, source, output) = setup();
    let nested = source.join("sub");
    fs::create_dir(&nested).expect("nested folder should be created");
    fs::write(nested.join("20261022.pdf"), b"aninhado").expect("fixture should be written");
    let configured_rule = rule(&source, &output, CheckMode::Instant);
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let mut monitor =
        Monitor::start_with_options(config(configured_rule.clone()), event_tx, test_options());
    let ignored = tokio::time::timeout(Duration::from_millis(200), event_rx.recv()).await;
    assert!(ignored.is_err(), "nested file should initially be ignored");

    let mut recursive_rule = configured_rule;
    recursive_rule.include_subfolders = true;
    monitor.reload(config(recursive_rule)).await;
    let (_, outcome) = next_processed(&mut event_rx).await;

    assert!(matches!(outcome, Outcome::Moved { .. }));
    assert!(output.join("2026/10/20261022.pdf").exists());
    monitor.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn overlapping_watchers_use_rule_priority() {
    let directory = tempdir().expect("temp directory should be created");
    let source = directory.path().join("entrada");
    let first_output = directory.path().join("primeira");
    let later_output = directory.path().join("posterior");
    fs::create_dir(&source).expect("source should be created");
    fs::create_dir(&first_output).expect("first destination should be created");
    fs::create_dir(&later_output).expect("later destination should be created");
    fs::write(source.join("20261022.pdf"), b"prioridade").expect("fixture should be written");
    let mut first = rule(&source, &first_output, CheckMode::Instant);
    first.priority = 10;
    let first_id = first.id;
    let mut later = rule(&source, &later_output, CheckMode::Instant);
    later.priority = 20;
    let configuration = Config {
        rules: vec![later, first],
        ..Config::default()
    };
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let mut monitor = Monitor::start_with_options(configuration, event_tx, test_options());

    let event = tokio::time::timeout(Duration::from_secs(4), event_rx.recv())
        .await
        .expect("event should arrive")
        .expect("event channel should remain open");

    assert!(matches!(
        event,
        MonitorEvent::FileProcessed { rule_id, .. } if rule_id == first_id
    ));
    assert!(first_output.join("2026/10/20261022.pdf").exists());
    assert!(!later_output.join("2026/10/20261022.pdf").exists());
    monitor.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unavailable_source_pauses_and_recovers_on_retry() {
    let directory = tempdir().expect("temp directory should be created");
    let source = directory.path().join("entrada-ainda-ausente");
    let output = directory.path().join("saida-ainda-ausente");
    let configured_rule = rule(&source, &output, CheckMode::Instant);
    let rule_id = configured_rule.id;
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let mut monitor =
        Monitor::start_with_options(config(configured_rule), event_tx, test_options());

    let paused = tokio::time::timeout(Duration::from_secs(2), event_rx.recv())
        .await
        .expect("pause event should arrive")
        .expect("event channel should remain open");
    assert!(matches!(
        paused,
        MonitorEvent::RulePaused { rule_id: paused_id, .. } if paused_id == rule_id
    ));

    fs::create_dir(&source).expect("source should become available");
    tokio::time::sleep(Duration::from_millis(150)).await;
    fs::write(source.join("20261022.pdf"), b"retomado").expect("fixture should be written");

    let (_, outcome) = next_processed(&mut event_rx).await;

    assert!(matches!(outcome, Outcome::Moved { .. }));
    assert_eq!(
        fs::read(output.join("2026/10/20261022.pdf")).expect("recovered rule should move file"),
        b"retomado"
    );
    monitor.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn non_recursive_rule_ignores_nested_file_but_moves_top_level_file() {
    let (_directory, source, output) = setup();
    let nested = source.join("sub");
    fs::create_dir(&nested).expect("nested folder should be created");
    fs::write(nested.join("20261021.pdf"), b"aninhado").expect("nested fixture should be written");
    fs::write(source.join("20261022.pdf"), b"topo").expect("top fixture should be written");
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let mut monitor = Monitor::start_with_options(
        config(rule(&source, &output, CheckMode::Instant)),
        event_tx,
        test_options(),
    );

    let (_, outcome) = next_processed(&mut event_rx).await;

    assert!(matches!(outcome, Outcome::Moved { .. }));
    assert!(nested.join("20261021.pdf").exists());
    assert!(output.join("2026/10/20261022.pdf").exists());
    let unexpected = tokio::time::timeout(Duration::from_millis(250), event_rx.recv()).await;
    assert!(
        unexpected.is_err(),
        "nested file was unexpectedly processed"
    );
    monitor.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn recursive_rule_moves_a_deeply_nested_file() {
    let (_directory, source, output) = setup();
    let nested = source.join("a/b");
    fs::create_dir_all(&nested).expect("nested folders should be created");
    fs::write(nested.join("20261022.pdf"), b"aninhado").expect("nested fixture should be written");
    let mut configured_rule = rule(&source, &output, CheckMode::Instant);
    configured_rule.include_subfolders = true;
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let mut monitor =
        Monitor::start_with_options(config(configured_rule), event_tx, test_options());

    let (file, outcome) = next_processed(&mut event_rx).await;

    assert_eq!(file, nested.join("20261022.pdf"));
    assert!(matches!(outcome, Outcome::Moved { .. }));
    assert!(output.join("2026/10/20261022.pdf").exists());
    assert!(nested.exists(), "empty source folders must remain");
    monitor.stop().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn recursive_rule_scans_a_folder_moved_into_the_source() {
    let (directory, source, output) = setup();
    let staged = directory.path().join("lote");
    fs::create_dir_all(staged.join("vazia")).expect("staged folders should be created");
    for (name, contents) in [
        ("20261021.pdf", b"um".as_slice()),
        ("20261022.pdf", b"dois".as_slice()),
        ("20261023.pdf", b"tres".as_slice()),
    ] {
        fs::write(staged.join(name), contents).expect("staged file should be written");
    }
    let mut configured_rule = rule(&source, &output, CheckMode::Instant);
    configured_rule.include_subfolders = true;
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let mut monitor =
        Monitor::start_with_options(config(configured_rule), event_tx, test_options());
    tokio::time::sleep(Duration::from_millis(100)).await;

    let imported = source.join("lote");
    fs::rename(&staged, &imported).expect("folder should move into source");
    for _ in 0..3 {
        let (_, outcome) = next_processed(&mut event_rx).await;
        assert!(matches!(outcome, Outcome::Moved { .. }));
    }

    for name in ["20261021.pdf", "20261022.pdf", "20261023.pdf"] {
        assert!(output.join("2026/10").join(name).exists());
    }
    assert!(imported.exists(), "source folder must not be deleted");
    assert!(
        imported.join("vazia").exists(),
        "empty subfolder must remain"
    );
    monitor.stop().await;
}

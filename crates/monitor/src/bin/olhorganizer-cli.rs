use std::env;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use monitor::{Monitor, MonitorEvent, MonitorOptions};
use motor::{Config, Outcome};
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<()> {
    let _ = tracing_subscriber::fmt().with_target(false).try_init();
    let arguments = parse_arguments()?;
    let config = Config::load(&arguments.config).with_context(|| {
        format!(
            "Não foi possível carregar a configuração '{}'.",
            arguments.config.display()
        )
    })?;
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let options = MonitorOptions {
        dry_run: arguments.dry_run,
        ..MonitorOptions::default()
    };
    let mut monitor = Monitor::start_with_options(config, event_tx, options);

    loop {
        tokio::select! {
            signal = tokio::signal::ctrl_c() => {
                signal.context("Não foi possível aguardar o Ctrl+C.")?;
                break;
            }
            event = event_rx.recv() => {
                let Some(event) = event else {
                    break;
                };
                println!("{}", format_event(event));
            }
        }
    }

    monitor.stop().await;
    Ok(())
}

struct Arguments {
    config: PathBuf,
    dry_run: bool,
}

fn parse_arguments() -> Result<Arguments> {
    let mut arguments = env::args_os().skip(1);
    let mut config = None;
    let mut dry_run = false;

    while let Some(argument) = arguments.next() {
        if argument == "--config" {
            let Some(path) = arguments.next() else {
                bail!("Informe um caminho depois de --config.");
            };
            config = Some(PathBuf::from(path));
        } else if argument == "--dry-run" {
            dry_run = true;
        } else if argument == "--help" || argument == "-h" {
            println!("Uso: olhorganizer-cli --config caminho/config.json [--dry-run]");
            std::process::exit(0);
        } else {
            bail!("Argumento desconhecido: {}", argument.to_string_lossy());
        }
    }

    let Some(config) = config else {
        bail!("Use --config para informar o arquivo de configuração.");
    };
    Ok(Arguments { config, dry_run })
}

fn format_event(event: MonitorEvent) -> String {
    match event {
        MonitorEvent::FileProcessed { file, outcome, .. } => {
            let file_name = display_file_name(&file);
            match outcome {
                Outcome::Moved { to } => format!(
                    "{file_name} movido para {}",
                    destination_directory(&to).display()
                ),
                Outcome::Copied { to } => format!(
                    "{file_name} copiado para {}",
                    destination_directory(&to).display()
                ),
                Outcome::WouldMove { to } => format!(
                    "{file_name} seria movido para {}",
                    destination_directory(&to).display()
                ),
                Outcome::WouldCopy { to } => format!(
                    "{file_name} seria copiado para {}",
                    destination_directory(&to).display()
                ),
                Outcome::Skipped { reason } => format!("{file_name} ignorado: {reason}"),
                Outcome::NotMatched => format!("{file_name} não corresponde ao filtro"),
                Outcome::Failed { error } => format!("Falha ao processar {file_name}: {error}"),
            }
        }
        MonitorEvent::RuleError {
            rule_id,
            message_pt,
        } => format!("Erro na regra {rule_id}: {message_pt}"),
        MonitorEvent::RulePaused { rule_id, reason_pt } => {
            format!("Regra {rule_id} pausada: {reason_pt}")
        }
    }
}

fn display_file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

fn destination_directory(path: &Path) -> &Path {
    path.parent().unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_moved_event_in_portuguese() {
        let line = format_event(MonitorEvent::FileProcessed {
            rule_id: motor::Uuid::new_v4(),
            file: PathBuf::from("20261022.pdf"),
            outcome: Outcome::Moved {
                to: PathBuf::from("/home/ana/Jornais/2026/10/20261022.pdf"),
            },
        });

        assert_eq!(line, "20261022.pdf movido para /home/ana/Jornais/2026/10");
    }
}

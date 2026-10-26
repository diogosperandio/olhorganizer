//! Command-line monitor: olhorganizer-cli --config config.json [--dry-run]

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, bail};
use monitor::{Monitor, MonitorOptions};
use motor::Config;
use tokio::sync::mpsc;

const USAGE: &str = "Uso: olhorganizer-cli --config caminho/para/config.json [--dry-run]";

struct Args {
    config: PathBuf,
    dry_run: bool,
}

fn parse_args() -> anyhow::Result<Args> {
    let mut config = None;
    let mut dry_run = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--config" => config = Some(PathBuf::from(args.next().context(USAGE)?)),
            "--dry-run" => dry_run = true,
            "-h" | "--help" => {
                println!("{USAGE}");
                std::process::exit(0);
            }
            other => bail!("Opção desconhecida: {other}\n{USAGE}"),
        }
    }
    Ok(Args {
        config: config.context(USAGE)?,
        dry_run,
    })
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();

    let args = parse_args()?;
    if !args.config.exists() {
        bail!(
            "Arquivo de configuração não encontrado: {}",
            args.config.display()
        );
    }
    let config = Config::load(&args.config)?;
    let names: HashMap<_, _> = config
        .rules
        .iter()
        .map(|r| (r.id, r.name.clone()))
        .collect();

    let (tx, mut rx) = mpsc::unbounded_channel();
    let options = MonitorOptions {
        dry_run: args.dry_run,
        ..MonitorOptions::default()
    };
    let mut monitor = Monitor::start_with(&config, tx, options);

    let enabled = config.rules.iter().filter(|r| r.enabled).count();
    let mode = if args.dry_run {
        " (simulação: nenhum arquivo será alterado)"
    } else {
        ""
    };
    println!("Monitorando {enabled} regra(s){mode}. Pressione Ctrl+C para sair.");

    loop {
        tokio::select! {
            Some(event) = rx.recv() => {
                let rule = names.get(&event.rule_id()).map_or("?", String::as_str);
                println!("[{rule}] {}", event.describe_pt());
            }
            result = tokio::signal::ctrl_c() => {
                result.context("Não foi possível escutar o Ctrl+C")?;
                break;
            }
        }
    }

    monitor.stop();
    println!("Monitoramento encerrado.");
    Ok(())
}

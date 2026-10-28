use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::rule::{destination_fixed_prefix, path_is_same_or_inside, paths_equal};
use crate::Rule;

const CURRENT_CONFIG_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    pub version: u32,
    pub rules: Vec<Rule>,
    pub settings: Settings,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConfigValidation {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

impl ConfigValidation {
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: CURRENT_CONFIG_VERSION,
            rules: Vec::new(),
            settings: Settings::default(),
        }
    }
}

impl Config {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, MotorError> {
        let path = path.as_ref();
        let contents = match fs::read(path) {
            Ok(contents) => contents,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(source) => return Err(MotorError::io("ler", path, source)),
        };

        match serde_json::from_slice(&contents) {
            Ok(config) => Ok(config),
            Err(source) => {
                let backup = backup_path(path);
                fs::copy(path, &backup).map_err(|backup_source| MotorError::Backup {
                    path: path.to_path_buf(),
                    backup: backup.clone(),
                    source: backup_source,
                })?;

                Err(MotorError::CorruptConfig {
                    path: path.to_path_buf(),
                    backup,
                    source,
                })
            }
        }
    }

    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), MotorError> {
        self.ensure_valid_for_persistence()?;
        let path = path.as_ref();
        let mut serialized = serde_json::to_vec_pretty(self).map_err(MotorError::Serialize)?;
        serialized.push(b'\n');

        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let file_name = path.file_name().ok_or_else(|| MotorError::InvalidPath {
            path: path.to_path_buf(),
        })?;
        let temp_path = parent.join(format!(
            ".{}.{}.tmp",
            file_name.to_string_lossy(),
            Uuid::new_v4()
        ));

        let result = (|| {
            let mut temp_file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&temp_path)
                .map_err(|source| {
                    MotorError::io("criar o arquivo temporário", &temp_path, source)
                })?;
            temp_file.write_all(&serialized).map_err(|source| {
                MotorError::io("gravar o arquivo temporário", &temp_path, source)
            })?;
            temp_file.sync_all().map_err(|source| {
                MotorError::io("sincronizar o arquivo temporário", &temp_path, source)
            })?;
            drop(temp_file);

            fs::rename(&temp_path, path)
                .map_err(|source| MotorError::io("substituir a configuração", path, source))
        })();

        if result.is_err() {
            let _ = fs::remove_file(&temp_path);
        }

        result
    }

    pub fn rules_by_priority(&self) -> Vec<&Rule> {
        let mut rules: Vec<_> = self.rules.iter().filter(|rule| rule.enabled).collect();
        rules.sort_by_key(|rule| rule.priority);
        rules
    }

    pub fn import(path: impl AsRef<Path>) -> Result<Self, MotorError> {
        let config = Self::load(path)?;
        config.ensure_valid_for_persistence()?;
        Ok(config)
    }

    pub fn validate_all(&self) -> ConfigValidation {
        let enabled: Vec<_> = self.rules.iter().filter(|rule| rule.enabled).collect();
        let mut edges = vec![vec![false; enabled.len()]; enabled.len()];

        for (producer_index, producing_rule) in enabled.iter().enumerate() {
            let destination = destination_fixed_prefix(&producing_rule.destination);
            let destination_has_markers = producing_rule.destination.contains('{');
            for (watcher_index, watching_rule) in enabled.iter().enumerate() {
                if producer_index == watcher_index {
                    continue;
                }
                let watched = if watching_rule.include_subfolders {
                    path_is_same_or_inside(&destination, &watching_rule.source_dir)
                } else {
                    !destination_has_markers && paths_equal(&destination, &watching_rule.source_dir)
                };
                if watched {
                    edges[producer_index][watcher_index] = true;
                }
            }
        }

        let reachability = transitive_closure(&edges);
        let mut validation = ConfigValidation::default();
        let mut included_in_cycle = vec![false; enabled.len()];
        for first in 0..enabled.len() {
            if included_in_cycle[first] {
                continue;
            }
            let cycle: Vec<_> = (0..enabled.len())
                .filter(|&candidate| {
                    candidate == first
                        || (reachability[first][candidate] && reachability[candidate][first])
                })
                .filter(|&candidate| {
                    candidate != first
                        || (0..enabled.len()).any(|other| {
                            other != first
                                && reachability[first][other]
                                && reachability[other][first]
                        })
                })
                .collect();
            if cycle.len() > 1 {
                for &index in &cycle {
                    included_in_cycle[index] = true;
                }
                let names: Vec<_> = cycle
                    .iter()
                    .map(|&index| enabled[index].name.as_str())
                    .collect();
                validation.errors.push(format!(
                    "As regras {} moveriam os arquivos de uma para a outra sem parar.",
                    format_rule_names(&names)
                ));
            }
        }

        for (producer_index, producing_rule) in enabled.iter().enumerate() {
            for (watcher_index, watching_rule) in enabled.iter().enumerate() {
                if edges[producer_index][watcher_index]
                    && !(reachability[watcher_index][producer_index])
                {
                    validation.warnings.push(format!(
                        "Os arquivos movidos pela regra '{}' serão processados de novo pela regra '{}'.",
                        producing_rule.name, watching_rule.name
                    ));
                }
            }
        }

        validation
    }

    fn ensure_valid_for_persistence(&self) -> Result<(), MotorError> {
        for rule in &self.rules {
            if let Some(problem) = rule.validate().into_iter().next() {
                return Err(MotorError::InvalidConfiguration {
                    message: format!("A regra '{}' não é válida: {problem}", rule.name),
                });
            }
        }
        if let Some(message) = self.validate_all().errors.into_iter().next() {
            return Err(MotorError::InvalidConfiguration { message });
        }
        Ok(())
    }
}

fn transitive_closure(edges: &[Vec<bool>]) -> Vec<Vec<bool>> {
    let mut reachable = edges.to_vec();
    for middle in 0..reachable.len() {
        for source in 0..reachable.len() {
            for destination in 0..reachable.len() {
                reachable[source][destination] |=
                    reachable[source][middle] && reachable[middle][destination];
            }
        }
    }
    reachable
}

fn format_rule_names(names: &[&str]) -> String {
    match names {
        [] => String::new(),
        [name] => format!("'{name}'"),
        [first, second] => format!("'{first}' e '{second}'"),
        _ => {
            let quoted: Vec<_> = names.iter().map(|name| format!("'{name}'")).collect();
            if let Some((last, first)) = quoted.split_last() {
                format!("{} e {last}", first.join(", "))
            } else {
                String::new()
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    pub check_updates_on_start: bool,
    pub skipped_version: Option<String>,
    pub notifications: bool,
    pub start_with_system: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            check_updates_on_start: true,
            skipped_version: None,
            notifications: true,
            start_with_system: false,
        }
    }
}

#[derive(Debug, Error)]
pub enum MotorError {
    #[error("Não foi possível {operation} '{path}': {source}")]
    Io {
        operation: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error(
        "O arquivo de configuração '{path}' está corrompido; uma cópia foi salva em '{backup}': {source}"
    )]
    CorruptConfig {
        path: PathBuf,
        backup: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error(
        "O arquivo de configuração '{path}' está corrompido e não foi possível criar a cópia de segurança '{backup}': {source}"
    )]
    Backup {
        path: PathBuf,
        backup: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("Não foi possível serializar a configuração: {0}")]
    Serialize(#[source] serde_json::Error),
    #[error("O caminho de configuração '{path}' não aponta para um arquivo.")]
    InvalidPath { path: PathBuf },
    #[error("Configuração inválida: {message}")]
    InvalidConfiguration { message: String },
    #[error("O marcador '{marker}' não é conhecido.")]
    UnknownMarker { marker: String },
    #[error("Há uma chave aberta sem fechamento na posição {position} do filtro.")]
    UnclosedBrace { position: usize },
    #[error("O nome de captura '{name}' está repetido no filtro.")]
    DuplicateCapture { name: String },
    #[error("O nome de captura '{name}' entra em conflito com um marcador padrão.")]
    BuiltinCaptureCollision { name: String },
    #[error("O caractere '|' é reservado para modificadores futuros e ainda não pode ser usado.")]
    ReservedModifier,
    #[error("O nome de captura '{name}' é inválido; use letras, números, '-' ou '_'.")]
    InvalidCaptureName { name: String },
    #[error("A lista de extensões '.{{{list}}}' é inválida.")]
    InvalidExtensionList { list: String },
    #[error("Não foi possível compilar o filtro: {0}")]
    InvalidFilter(#[source] regex::Error),
    #[error("A regra usa a data do nome, mas o filtro não capturou uma data completa.")]
    MissingFileNameDate,
    #[error("A data capturada no nome do arquivo é inválida.")]
    InvalidCapturedDate,
    #[error("O arquivo de exemplo '{file_name}' não corresponde ao filtro da regra.")]
    SampleNotMatched { file_name: String },
    #[error("Não foi possível ler a data de modificação do arquivo: {0}")]
    ModifiedTime(#[source] std::io::Error),
    #[error("Há uma chave aberta sem fechamento no destino na posição {position}.")]
    UnclosedDestinationMarker { position: usize },
}

impl MotorError {
    fn io(operation: &'static str, path: &Path, source: std::io::Error) -> Self {
        Self::Io {
            operation,
            path: path.to_path_buf(),
            source,
        }
    }
}

fn backup_path(path: &Path) -> PathBuf {
    let mut backup = path.as_os_str().to_owned();
    backup.push(".bak");
    PathBuf::from(backup)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;
    use crate::{Action, CheckMode, ConflictPolicy, DateSource};

    fn rule(name: &str, priority: u32, enabled: bool) -> Rule {
        Rule {
            id: Uuid::new_v4(),
            name: name.to_owned(),
            enabled,
            source_dir: PathBuf::from("/entrada"),
            filter: "*.pdf".to_owned(),
            destination: "/saida/{YYYY}".to_owned(),
            action: Action::Move,
            date_source: DateSource::FileName,
            check_mode: CheckMode::Instant,
            include_subfolders: false,
            on_conflict: ConflictPolicy::Rename,
            priority,
        }
    }

    #[test]
    fn saves_and_loads_round_trip() {
        let directory = tempdir().expect("temp directory should be created");
        let path = directory.path().join("config.json");
        let config = Config {
            rules: vec![rule("PDFs", 10, true)],
            ..Config::default()
        };

        config.save(&path).expect("config should be saved");
        let loaded = Config::load(&path).expect("config should be loaded");

        assert_eq!(loaded, config);
    }

    #[test]
    fn missing_file_returns_default_config() {
        let directory = tempdir().expect("temp directory should be created");
        let path = directory.path().join("missing.json");

        assert_eq!(
            Config::load(path).expect("missing is valid"),
            Config::default()
        );
    }

    #[test]
    fn corrupt_file_returns_error_and_creates_backup() {
        let directory = tempdir().expect("temp directory should be created");
        let path = directory.path().join("config.json");
        fs::write(&path, b"{ invalid json").expect("fixture should be written");

        let error = Config::load(&path).expect_err("corrupt JSON should fail");

        assert!(matches!(error, MotorError::CorruptConfig { .. }));
        assert_eq!(
            fs::read(directory.path().join("config.json.bak")).expect("backup should be readable"),
            b"{ invalid json"
        );
    }

    #[test]
    fn returns_only_enabled_rules_in_priority_order() {
        let config = Config {
            rules: vec![
                rule("Terceira", 30, true),
                rule("Desativada", 1, false),
                rule("Primeira", 10, true),
                rule("Segunda", 20, true),
            ],
            ..Config::default()
        };

        let names: Vec<_> = config
            .rules_by_priority()
            .into_iter()
            .map(|rule| rule.name.as_str())
            .collect();

        assert_eq!(names, ["Primeira", "Segunda", "Terceira"]);
    }

    #[test]
    fn example_config_loads() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/config.example.json");
        let config = Config::load(path).expect("example config should load");

        assert_eq!(config.rules.len(), 3);
    }

    #[test]
    fn example_news_rule_only_matches_pdfs_with_a_complete_date() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/config.example.json");
        let config = Config::load(path).expect("example config should load");
        let news_rule = config.rules.first().expect("news rule should exist");
        let filter = crate::compile(&news_rule.filter).expect("news filter should compile");

        let captures = filter
            .matches("20261022.pdf")
            .expect("dated PDF should match");
        assert_eq!(captures.year.as_deref(), Some("2026"));
        assert_eq!(captures.month.as_deref(), Some("10"));
        assert_eq!(captures.day.as_deref(), Some("22"));
        assert!(filter.matches("canela.pdf").is_none());
        assert!(filter.matches("bIaubi.pdf").is_none());
    }

    #[test]
    fn old_config_defaults_include_subfolders_to_false() {
        let json = r#"{
            "version": 1,
            "rules": [{
                "id": "bbf2fcf0-7bcf-4d9a-b1f8-bccf3f720199",
                "name": "Antiga",
                "enabled": true,
                "source_dir": "/entrada",
                "filter": "*.pdf",
                "destination": "/saida",
                "action": "move",
                "date_source": "modified",
                "check_mode": "instant",
                "on_conflict": "rename",
                "priority": 1
            }],
            "settings": {
                "check_updates_on_start": true,
                "skipped_version": null,
                "notifications": true,
                "start_with_system": false
            }
        }"#;

        let config: Config = serde_json::from_str(json).expect("old config should load");

        assert!(!config.rules[0].include_subfolders);
    }

    #[test]
    fn warns_when_a_destination_is_watched_by_another_rule() {
        let mut producing = rule("A", 1, true);
        producing.source_dir = PathBuf::from("/origem");
        producing.destination = "/monitorada/organizados/{YYYY}".to_owned();
        let mut watching = rule("B", 2, true);
        watching.source_dir = PathBuf::from("/monitorada");
        watching.include_subfolders = true;
        watching.destination = "/outra-saida".to_owned();
        let config = Config {
            rules: vec![producing, watching],
            ..Config::default()
        };

        assert_eq!(
            config.validate_all().warnings,
            ["Os arquivos movidos pela regra 'A' serão processados de novo pela regra 'B'."]
        );
    }

    fn routed_rule(name: &str, source: &str, destination: &str) -> Rule {
        let mut configured_rule = rule(name, 1, true);
        configured_rule.source_dir = PathBuf::from(source);
        configured_rule.destination = destination.to_owned();
        configured_rule
    }

    #[test]
    fn rejects_a_two_rule_cycle() {
        let config = Config {
            rules: vec![routed_rule("A", "/a", "/b"), routed_rule("B", "/b", "/a")],
            ..Config::default()
        };

        assert_eq!(
            config.validate_all().errors,
            ["As regras 'A' e 'B' moveriam os arquivos de uma para a outra sem parar."]
        );
    }

    #[test]
    fn allows_nested_destinations_below_the_same_non_recursive_source() {
        let mut retino = routed_rule(
            "retino",
            "/home/diogo/Downloads/NEWS",
            "/home/diogo/Downloads/NEWS/{YYYY}/{MM}",
        );
        retino.filter = "*{YYYY}{MM}{DD}*.*".to_owned();

        let mut regra2 = routed_rule(
            "regra2",
            "/home/diogo/Downloads/NEWS",
            "/home/diogo/Downloads/NEWS/{YYYY}/{ne}/{MM}",
        );
        regra2.filter = "*{TEXTO:ne}*.jpg".to_owned();

        let config = Config {
            rules: vec![retino, regra2],
            ..Config::default()
        };

        assert_eq!(config.validate_all(), ConfigValidation::default());
    }

    #[test]
    fn rejects_marker_destinations_in_a_recursive_rule_cycle() {
        let mut first = routed_rule("A", "/a", "/b/{YYYY}");
        first.include_subfolders = true;
        let mut second = routed_rule("B", "/b", "/a/{YYYY}");
        second.include_subfolders = true;
        let config = Config {
            rules: vec![first, second],
            ..Config::default()
        };

        assert_eq!(
            config.validate_all().errors,
            ["As regras 'A' e 'B' moveriam os arquivos de uma para a outra sem parar."]
        );
    }

    #[test]
    fn rejects_a_three_rule_cycle() {
        let config = Config {
            rules: vec![
                routed_rule("A", "/a", "/b"),
                routed_rule("B", "/b", "/c"),
                routed_rule("C", "/c", "/a"),
            ],
            ..Config::default()
        };

        assert_eq!(
            config.validate_all().errors,
            ["As regras 'A', 'B' e 'C' moveriam os arquivos de uma para a outra sem parar."]
        );
    }

    #[test]
    fn accepts_a_chain_with_a_warning() {
        let config = Config {
            rules: vec![
                routed_rule("A", "/a", "/b"),
                routed_rule("B", "/b", "/c"),
                routed_rule("C", "/c", "/fora"),
            ],
            ..Config::default()
        };
        let validation = config.validate_all();

        assert!(validation.errors.is_empty());
        assert_eq!(validation.warnings.len(), 2);
    }

    #[test]
    fn import_rejects_a_rule_cycle_but_load_allows_manual_recovery() {
        let directory = tempdir().expect("temp directory should be created");
        let path = directory.path().join("config.json");
        let config = Config {
            rules: vec![routed_rule("A", "/a", "/b"), routed_rule("B", "/b", "/a")],
            ..Config::default()
        };
        fs::write(
            &path,
            serde_json::to_vec(&config).expect("config should serialize"),
        )
        .expect("fixture should be written");

        assert!(matches!(
            Config::import(&path),
            Err(MotorError::InvalidConfiguration { .. })
        ));
        assert_eq!(
            Config::load(&path)
                .expect("manually edited config should still load")
                .rules
                .len(),
            2
        );
    }

    #[test]
    fn save_rejects_a_rule_cycle() {
        let directory = tempdir().expect("temp directory should be created");
        let config = Config {
            rules: vec![routed_rule("A", "/a", "/b"), routed_rule("B", "/b", "/a")],
            ..Config::default()
        };

        assert!(matches!(
            config.save(directory.path().join("config.json")),
            Err(MotorError::InvalidConfiguration { .. })
        ));
    }
}

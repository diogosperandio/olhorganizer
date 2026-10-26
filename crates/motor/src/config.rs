//! Application configuration and its JSON persistence.

use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::MotorError;
use crate::rule::{Action, Rule};

/// A rule combination that works but is probably a mistake.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleWarning {
    /// The rule whose destination is the problem.
    pub rule_id: Uuid,
    /// The rule that would pick its files up again.
    pub other_rule_id: Uuid,
    pub message_pt: String,
}

/// Current version of the config file format.
pub const CONFIG_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub version: u32,
    pub rules: Vec<Rule>,
    pub settings: Settings,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            version: CONFIG_VERSION,
            rules: Vec::new(),
            settings: Settings::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub check_updates_on_start: bool,
    /// Update version the user chose not to install.
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

impl Config {
    /// Loads the config from `path`.
    ///
    /// A missing file gives [`Config::default`]. A file that cannot be parsed
    /// is copied to `<name>.bak` next to it and reported as
    /// [`MotorError::CorruptConfig`]; the original is left untouched.
    pub fn load(path: &Path) -> Result<Self, MotorError> {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(source) => return Err(io_error(path, source)),
        };

        serde_json::from_slice(&bytes).map_err(|source| {
            let backup = with_suffix(path, ".bak");
            let backup = fs::copy(path, &backup).ok().map(|_| backup);
            MotorError::CorruptConfig {
                path: path.to_path_buf(),
                backup,
                source,
            }
        })
    }

    /// Saves the config to `path`, creating its folder if needed.
    ///
    /// The JSON is written to `<name>.tmp` and then renamed over `path`, so a
    /// crash never leaves a half-written config.
    pub fn save(&self, path: &Path) -> Result<(), MotorError> {
        let json = serde_json::to_vec_pretty(self).map_err(MotorError::Serialize)?;

        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent).map_err(|source| io_error(parent, source))?;
        }

        let tmp = with_suffix(path, ".tmp");
        let result = write_synced(&tmp, &json)
            .map_err(|source| io_error(&tmp, source))
            .and_then(|()| fs::rename(&tmp, path).map_err(|source| io_error(path, source)));
        if result.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        result
    }

    /// Enabled rules, lowest priority value first. Ties keep their file order.
    /// Warns when a rule's destination lies in the area another enabled rule
    /// watches (its folder, or its whole tree with subfolders), so files would
    /// be processed twice. A rule overlapping itself is an error reported by
    /// [`Rule::validate`] instead.
    pub fn validate_all(&self) -> Vec<RuleWarning> {
        let enabled: Vec<&Rule> = self.rules.iter().filter(|r| r.enabled).collect();
        let mut warnings = Vec::new();
        for rule in &enabled {
            if !rule.destination_prefix().is_absolute() {
                continue;
            }
            for other in &enabled {
                if other.id == rule.id || !rule.destination_reaches(other) {
                    continue;
                }
                let verb = match rule.action {
                    Action::Move => "movidos",
                    Action::Copy => "copiados",
                };
                warnings.push(RuleWarning {
                    rule_id: rule.id,
                    other_rule_id: other.id,
                    message_pt: format!(
                        "Os arquivos {verb} pela regra '{}' serão processados de novo pela regra '{}'.",
                        rule.name, other.name
                    ),
                });
            }
        }
        warnings
    }

    pub fn rules_by_priority(&self) -> Vec<&Rule> {
        let mut rules: Vec<&Rule> = self.rules.iter().filter(|r| r.enabled).collect();
        rules.sort_by_key(|r| r.priority);
        rules
    }
}

fn write_synced(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

/// `config.json` + `.bak` -> `config.json.bak`, in the same folder.
fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.file_name().map(OsString::from).unwrap_or_default();
    name.push(suffix);
    path.with_file_name(name)
}

fn io_error(path: &Path, source: io::Error) -> MotorError {
    MotorError::Io {
        path: path.to_path_buf(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rule::{CheckMode, ConflictPolicy, DateSource};

    fn rule(name: &str, priority: u32, enabled: bool) -> Rule {
        Rule {
            id: Uuid::new_v4(),
            name: name.into(),
            enabled,
            source_dir: PathBuf::from("/home/u/NEWS"),
            include_subfolders: false,
            filter: "*.pdf".into(),
            destination: "/home/u/Jornais/{YYYY}/{MM}".into(),
            action: Action::Move,
            date_source: DateSource::FileName,
            check_mode: CheckMode::Interval { minutes: 5 },
            on_conflict: ConflictPolicy::Skip,
            priority,
        }
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let config = Config {
            version: CONFIG_VERSION,
            rules: vec![rule("a", 1, true), rule("b", 2, false)],
            settings: Settings {
                check_updates_on_start: false,
                skipped_version: Some("1.2.0".into()),
                notifications: false,
                start_with_system: true,
            },
        };

        config.save(&path).unwrap();
        assert_eq!(Config::load(&path).unwrap(), config);
    }

    #[test]
    fn save_replaces_existing_file_and_leaves_no_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        Config::default().save(&path).unwrap();

        let mut config = Config::default();
        config.rules.push(rule("a", 0, true));
        config.save(&path).unwrap();

        assert_eq!(Config::load(&path).unwrap(), config);
        assert!(!dir.path().join("config.json.tmp").exists());
    }

    #[test]
    fn save_creates_missing_folders() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a").join("b").join("config.json");
        Config::default().save(&path).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn missing_file_gives_default() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::load(&dir.path().join("config.json")).unwrap();
        assert_eq!(config, Config::default());
    }

    #[test]
    fn missing_fields_use_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        fs::write(&path, r#"{ "version": 1 }"#).unwrap();
        assert_eq!(Config::load(&path).unwrap(), Config::default());
    }

    #[test]
    fn corrupt_file_is_reported_and_backed_up() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let content = r#"{ "version": 1, "rules": [ "#;
        fs::write(&path, content).unwrap();

        let err = Config::load(&path).unwrap_err();
        let MotorError::CorruptConfig { backup, .. } = err else {
            panic!("expected CorruptConfig, got {err:?}");
        };
        let backup = backup.expect("backup should be written");
        assert_eq!(backup, dir.path().join("config.json.bak"));
        assert_eq!(fs::read_to_string(&backup).unwrap(), content);
        assert_eq!(fs::read_to_string(&path).unwrap(), content);
    }

    #[test]
    fn rule_without_include_subfolders_loads_with_it_off() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let old = r#"{
            "version": 1,
            "rules": [{
                "id": "6f1c2a3e-8b4d-4e5f-9a10-2b3c4d5e6f70",
                "name": "Jornais",
                "enabled": true,
                "source_dir": "/home/u/NEWS",
                "filter": "{YYYY}{MM}{DD}.pdf",
                "destination": "/home/u/Jornais/{YYYY}/{MM}",
                "action": "move",
                "date_source": "file_name",
                "check_mode": { "mode": "instant" },
                "on_conflict": "rename",
                "priority": 1
            }]
        }"#;
        fs::write(&path, old).unwrap();
        let config = Config::load(&path).unwrap();
        assert!(!config.rules[0].include_subfolders);
    }

    #[test]
    fn warns_when_a_destination_feeds_another_rule() {
        // Paths need not exist: the check is textual.
        let rule_at = |name: &str, source: &str, destination: &str, subfolders: bool| Rule {
            source_dir: PathBuf::from(source),
            destination: destination.into(),
            include_subfolders: subfolders,
            ..rule(name, 0, true)
        };
        let a = rule_at("A", "/home/u/NEWS", "/home/u/Arquivo/{YYYY}", false);
        let b_tree = rule_at("B", "/home/u/Arquivo", "/home/u/Fora", true);
        let b_top = rule_at("B", "/home/u/Arquivo", "/home/u/Fora", false);

        let config = Config {
            rules: vec![a.clone(), b_tree.clone()],
            ..Config::default()
        };
        let warnings = config.validate_all();
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            (warnings[0].rule_id, warnings[0].other_rule_id),
            (a.id, b_tree.id)
        );
        assert_eq!(
            warnings[0].message_pt,
            "Os arquivos movidos pela regra 'A' serão processados de novo pela regra 'B'."
        );

        // B watches only its top level; A's files go into a year subfolder.
        let config = Config {
            rules: vec![a.clone(), b_top.clone()],
            ..Config::default()
        };
        assert!(config.validate_all().is_empty());

        // ...unless A drops them straight into B's folder.
        let a_flat = Rule {
            destination: "/home/u/Arquivo".into(),
            ..a.clone()
        };
        let config = Config {
            rules: vec![a_flat, b_top],
            ..Config::default()
        };
        assert_eq!(config.validate_all().len(), 1);

        // Disabled rules do not count.
        let config = Config {
            rules: vec![
                a,
                Rule {
                    enabled: false,
                    ..b_tree
                },
            ],
            ..Config::default()
        };
        assert!(config.validate_all().is_empty());
    }

    #[test]
    fn rules_by_priority_sorts_enabled_rules() {
        let config = Config {
            rules: vec![
                rule("c", 3, true),
                rule("off", 0, false),
                rule("a", 1, true),
                rule("b1", 2, true),
                rule("b2", 2, true),
            ],
            ..Config::default()
        };
        let names: Vec<&str> = config
            .rules_by_priority()
            .iter()
            .map(|r| r.name.as_str())
            .collect();
        assert_eq!(names, ["a", "b1", "b2", "c"]);
    }

    #[test]
    fn example_config_loads() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/config.example.json");
        let config = Config::load(&path).unwrap();
        assert_eq!(config.rules.len(), 3);
        assert!(config.rules.iter().any(|r| r.include_subfolders));
        assert_eq!(config.validate_all(), []);
        if cfg!(unix) {
            for rule in &config.rules {
                assert!(
                    rule.validate().is_empty(),
                    "{}: {:?}",
                    rule.name,
                    rule.validate()
                );
            }
        }
    }
}

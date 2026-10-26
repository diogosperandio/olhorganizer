//! Application configuration and its JSON persistence.

use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::MotorError;
use crate::rule::Rule;

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
    use crate::rule::{Action, CheckMode, ConflictPolicy, DateSource};
    use uuid::Uuid;

    fn rule(name: &str, priority: u32, enabled: bool) -> Rule {
        Rule {
            id: Uuid::new_v4(),
            name: name.into(),
            enabled,
            source_dir: PathBuf::from("/home/u/NEWS"),
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
        assert_eq!(config.rules.len(), 2);
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

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

/// Rules that would hand files to each other forever.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleCycle {
    pub rule_ids: Vec<Uuid>,
    pub message_pt: String,
}

/// Result of [`Config::validate_all`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConfigReport {
    /// Blocking: a config with cycles must not be saved.
    pub cycles: Vec<RuleCycle>,
    pub warnings: Vec<RuleWarning>,
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

    /// Checks how the enabled rules feed each other. There is an edge A -> B
    /// when A's destination lies in the area B watches (B's folder, or its
    /// whole tree with subfolders). Cycles are errors: files would bounce
    /// between the rules forever. Other edges are warnings: files are
    /// processed twice. A rule overlapping itself is reported by
    /// [`Rule::validate`] instead.
    pub fn validate_all(&self) -> ConfigReport {
        let rules: Vec<&Rule> = self
            .rules
            .iter()
            .filter(|r| r.enabled && r.destination_prefix().is_absolute())
            .collect();
        let edges: Vec<Vec<usize>> = rules
            .iter()
            .enumerate()
            .map(|(a, from)| {
                rules
                    .iter()
                    .enumerate()
                    .filter(|&(b, to)| a != b && from.destination_reaches(to))
                    .map(|(b, _)| b)
                    .collect()
            })
            .collect();
        let reach: Vec<Vec<bool>> = (0..rules.len()).map(|a| reachable(&edges, a)).collect();

        // Rules that can reach each other form a cycle.
        let mut cycle_of: Vec<Option<usize>> = vec![None; rules.len()];
        let mut cycles = Vec::new();
        for a in 0..rules.len() {
            if cycle_of[a].is_some() || !reach[a][a] {
                continue;
            }
            let members: Vec<usize> = (0..rules.len())
                .filter(|&b| reach[a][b] && reach[b][a])
                .collect();
            for &m in &members {
                cycle_of[m] = Some(cycles.len());
            }
            let names: Vec<&str> = members.iter().map(|&m| rules[m].name.as_str()).collect();
            cycles.push(RuleCycle {
                rule_ids: members.iter().map(|&m| rules[m].id).collect(),
                message_pt: cycle_message(&names),
            });
        }

        let mut warnings = Vec::new();
        for (a, targets) in edges.iter().enumerate() {
            for &b in targets {
                if cycle_of[a].is_some() && cycle_of[a] == cycle_of[b] {
                    continue;
                }
                let verb = match rules[a].action {
                    Action::Move => "movidos",
                    Action::Copy => "copiados",
                };
                warnings.push(RuleWarning {
                    rule_id: rules[a].id,
                    other_rule_id: rules[b].id,
                    message_pt: format!(
                        "Os arquivos {verb} pela regra '{}' serão processados de novo pela regra '{}'.",
                        rules[a].name, rules[b].name
                    ),
                });
            }
        }
        ConfigReport { cycles, warnings }
    }

    /// Adds `rule`, or replaces the rule with the same id, if the rule is
    /// valid and creates no cycle between rules. Returns the remaining
    /// warnings. Changes only this value; call [`Config::save`] to write it.
    pub fn save_rule(&mut self, rule: Rule) -> Result<Vec<RuleWarning>, MotorError> {
        let mut candidate = self.clone();
        candidate.put_rule(rule)?;
        candidate.replace(self)
    }

    /// Imports the rules of another config file, replacing rules with the
    /// same id and adding the others; settings are not imported. Nothing
    /// changes if any rule is invalid or the result has a cycle.
    pub fn import(&mut self, path: &Path) -> Result<Vec<RuleWarning>, MotorError> {
        let bytes = fs::read(path).map_err(|source| io_error(path, source))?;
        let imported: Config =
            serde_json::from_slice(&bytes).map_err(|source| MotorError::InvalidImport {
                path: path.to_path_buf(),
                source,
            })?;
        let mut candidate = self.clone();
        for rule in imported.rules {
            candidate.put_rule(rule)?;
        }
        candidate.replace(self)
    }

    fn put_rule(&mut self, rule: Rule) -> Result<(), MotorError> {
        let problems = rule.validate();
        if !problems.is_empty() {
            return Err(MotorError::InvalidRule {
                name: rule.name,
                problems,
            });
        }
        match self.rules.iter_mut().find(|r| r.id == rule.id) {
            Some(existing) => *existing = rule,
            None => self.rules.push(rule),
        }
        Ok(())
    }

    /// Becomes `target` unless the rules form a cycle.
    fn replace(self, target: &mut Config) -> Result<Vec<RuleWarning>, MotorError> {
        let report = self.validate_all();
        if !report.cycles.is_empty() {
            return Err(MotorError::RuleCycles(report.cycles));
        }
        *target = self;
        Ok(report.warnings)
    }

    /// Enabled rules, lowest priority value first. Ties keep their file order.
    pub fn rules_by_priority(&self) -> Vec<&Rule> {
        let mut rules: Vec<&Rule> = self.rules.iter().filter(|r| r.enabled).collect();
        rules.sort_by_key(|r| r.priority);
        rules
    }
}

/// Which nodes can be reached from `start` in one or more steps.
fn reachable(edges: &[Vec<usize>], start: usize) -> Vec<bool> {
    let mut seen = vec![false; edges.len()];
    let mut stack = edges[start].clone();
    while let Some(node) = stack.pop() {
        if !std::mem::replace(&mut seen[node], true) {
            stack.extend(&edges[node]);
        }
    }
    seen
}

fn cycle_message(names: &[&str]) -> String {
    let quoted: Vec<String> = names.iter().map(|n| format!("'{n}'")).collect();
    match quoted.as_slice() {
        [a, b] => {
            format!("As regras {a} e {b} moveriam os arquivos de uma para a outra sem parar.")
        }
        [rest @ .., last] => format!(
            "As regras {} e {last} moveriam os arquivos entre si sem parar.",
            rest.join(", ")
        ),
        [] => String::new(),
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

    fn abs(rel: &str) -> String {
        if cfg!(windows) {
            format!("C:\\{}", rel.replace('/', "\\"))
        } else {
            format!("/{rel}")
        }
    }

    /// A rule watching `source` and sending files to `destination`
    /// (`/`-separated, made absolute). The folders need not exist.
    fn rule_at(name: &str, source: &str, destination: &str, subfolders: bool) -> Rule {
        Rule {
            source_dir: PathBuf::from(abs(source)),
            destination: abs(destination),
            include_subfolders: subfolders,
            date_source: DateSource::Detected,
            ..rule(name, 0, true)
        }
    }

    fn config(rules: &[&Rule]) -> Config {
        Config {
            rules: rules.iter().map(|&r| r.clone()).collect(),
            ..Config::default()
        }
    }

    fn cycle_messages(config: &Config) -> Vec<String> {
        config
            .validate_all()
            .cycles
            .into_iter()
            .map(|c| c.message_pt)
            .collect()
    }

    #[test]
    fn chain_is_allowed_with_a_warning() {
        let a = rule_at("A", "u/NEWS", "u/Arquivo/{YYYY}", false);
        let b_tree = rule_at("B", "u/Arquivo", "u/Fora", true);
        let report = config(&[&a, &b_tree]).validate_all();
        assert_eq!(report.cycles, []);
        assert_eq!(report.warnings.len(), 1);
        let warning = &report.warnings[0];
        assert_eq!((warning.rule_id, warning.other_rule_id), (a.id, b_tree.id));
        assert_eq!(
            warning.message_pt,
            "Os arquivos movidos pela regra 'A' serão processados de novo pela regra 'B'."
        );

        // B watches only its top level; A's files go into a year subfolder.
        let b_top = rule_at("B", "u/Arquivo", "u/Fora", false);
        assert_eq!(
            config(&[&a, &b_top]).validate_all(),
            ConfigReport::default()
        );

        // ...unless A drops them straight into B's folder.
        let a_flat = rule_at("A", "u/NEWS", "u/Arquivo", false);
        assert_eq!(config(&[&a_flat, &b_top]).validate_all().warnings.len(), 1);

        // Disabled rules do not count.
        let b_off = Rule {
            enabled: false,
            ..b_tree.clone()
        };
        assert_eq!(
            config(&[&a, &b_off]).validate_all(),
            ConfigReport::default()
        );

        // save_rule accepts the chain and hands back the warning.
        let mut cfg = config(&[&a]);
        assert_eq!(cfg.save_rule(b_tree).unwrap().len(), 1);
        assert_eq!(cfg.rules.len(), 2);
    }

    #[test]
    fn two_rule_cycle_is_rejected() {
        // A drops files straight into B's folder and B into A's.
        let a = rule_at("A", "u/A", "u/B", false);
        let b = rule_at("B", "u/B", "u/A", false);
        // With subfolders on, B's year folders inside A's tree loop as well.
        let a_tree = rule_at("A", "u/A", "u/B", true);
        let b_into_tree = rule_at("B", "u/B", "u/A/{YYYY}", false);
        assert_eq!(cycle_messages(&config(&[&a_tree, &b_into_tree])).len(), 1);
        // ...but not when A only watches its top level.
        assert_eq!(cycle_messages(&config(&[&a, &b_into_tree])).len(), 0);

        assert_eq!(
            cycle_messages(&config(&[&a, &b])),
            ["As regras 'A' e 'B' moveriam os arquivos de uma para a outra sem parar."]
        );

        let mut cfg = config(&[&a]);
        let before = cfg.clone();
        let err = cfg.save_rule(b).unwrap_err();
        assert!(
            matches!(err, MotorError::RuleCycles(ref c) if c.len() == 1),
            "{err:?}"
        );
        assert_eq!(
            err.to_string(),
            "As regras 'A' e 'B' moveriam os arquivos de uma para a outra sem parar."
        );
        assert_eq!(cfg, before);
    }

    #[test]
    fn three_rule_cycle_is_rejected() {
        let a = rule_at("A", "u/A", "u/B", false);
        let b = rule_at("B", "u/B", "u/C", false);
        let c = rule_at("C", "u/C", "u/A", false);
        let report = config(&[&a, &b, &c]).validate_all();
        assert_eq!(report.cycles.len(), 1);
        assert_eq!(report.cycles[0].rule_ids, [a.id, b.id, c.id]);
        assert_eq!(
            report.cycles[0].message_pt,
            "As regras 'A', 'B' e 'C' moveriam os arquivos entre si sem parar."
        );
        // Edges inside the cycle are not repeated as warnings.
        assert_eq!(report.warnings, []);

        let mut cfg = config(&[&a, &b]);
        assert!(matches!(cfg.save_rule(c), Err(MotorError::RuleCycles(_))));
        assert_eq!(cfg.rules.len(), 2);
    }

    #[test]
    fn import_that_creates_a_cycle_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let a = rule_at("A", "u/A", "u/B", false);
        let b = rule_at("B", "u/B", "u/A", false);
        let exported = dir.path().join("regras.json");
        config(&[&b]).save(&exported).unwrap();

        let mut cfg = config(&[&a]);
        let before = cfg.clone();
        assert!(matches!(
            cfg.import(&exported),
            Err(MotorError::RuleCycles(_))
        ));
        assert_eq!(cfg, before);

        // Without the cycle the same import goes through.
        let mut cfg = Config::default();
        assert_eq!(cfg.import(&exported).unwrap(), []);
        assert_eq!(cfg.rules, [b]);
    }

    #[test]
    fn save_rule_rejects_an_invalid_rule() {
        let mut cfg = Config::default();
        let bad = rule_at("Ruim", "u/NEWS", "u/NEWS", false);
        let err = cfg.save_rule(bad).unwrap_err();
        assert!(matches!(err, MotorError::InvalidRule { .. }), "{err:?}");
        assert!(err.to_string().contains("'Ruim'"), "{err}");
        assert!(cfg.rules.is_empty());
    }

    #[test]
    fn import_of_a_broken_file_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("regras.json");
        fs::write(&path, "não é json").unwrap();
        let mut cfg = Config::default();
        assert!(matches!(
            cfg.import(&path),
            Err(MotorError::InvalidImport { .. })
        ));
        assert!(matches!(
            cfg.import(&dir.path().join("sumiu.json")),
            Err(MotorError::Io { .. })
        ));
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
        assert_eq!(config.validate_all(), ConfigReport::default());
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

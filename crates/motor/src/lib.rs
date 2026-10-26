//! Olhorganizer engine: rules, markers, filters and file operations.
//! Has no Tauri dependency.

mod config;
pub mod destination;
mod error;
mod filter;
mod markers;
pub mod ops;
pub mod plan;
mod rule;
#[cfg(test)]
mod testutil;

pub use config::{CONFIG_VERSION, Config, RuleWarning, Settings};
pub use destination::{DestinationError, FileMeta, build_destination};
pub use error::MotorError;
pub use filter::{
    Captures, Filter, FilterError, FilterTemplate, compile, filter_templates, is_temporary,
};
pub use markers::{
    MONTH_NAMES_PT, MarkerDef, MarkerKind, find_marker, list_markers, month_from_name,
};
pub use ops::{FileSystem, Outcome, StdFs, process};
pub use plan::{pick_rule, test_rule};
pub use uuid::Uuid;

pub use rule::{
    Action, CheckMode, ConflictPolicy, DateSource, Rule, RuleProblem, is_ignored_folder,
};

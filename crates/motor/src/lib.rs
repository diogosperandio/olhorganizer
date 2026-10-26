mod config;
mod destination;
mod filter;
mod markers;
pub mod ops;
mod plan;
mod rule;

pub use config::{Config, ConfigValidation, MotorError, Settings};
pub use destination::{build_destination, preview_destination};
pub use filter::{compile, list_filter_templates, Captures, Filter, FilterTemplate};
pub use markers::{list_markers, MarkerDef, MarkerKind};
pub use ops::{pick_rule, process, process_with_fs, FileSystem, Outcome, StdFileSystem};
pub use plan::{test_rule, test_rule_with_fs};
pub use rule::{Action, CheckMode, ConflictPolicy, DateSource, Rule};
pub use uuid::Uuid;

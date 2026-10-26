//! Helpers shared by tests.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Every entry under `root` (relative path -> file bytes, or `None` for
/// folders and symlinks), to check that a test left the disk unchanged.
pub fn snapshot(root: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
    let mut entries = BTreeMap::new();
    walk(root, root, &mut entries);
    entries
}

fn walk(root: &Path, dir: &Path, entries: &mut BTreeMap<PathBuf, Option<Vec<u8>>>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let relative = path.strip_prefix(root).unwrap().to_path_buf();
        let kind = fs::symlink_metadata(&path).unwrap().file_type();
        if kind.is_file() {
            entries.insert(relative, Some(fs::read(&path).unwrap()));
        } else {
            entries.insert(relative, None);
            if kind.is_dir() {
                walk(root, &path, entries);
            }
        }
    }
}

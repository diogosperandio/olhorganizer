use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use motor::Rule;
use walkdir::{DirEntry, WalkDir};

pub(crate) fn scan_files(directory: &Path, recursive: bool) -> io::Result<Vec<PathBuf>> {
    let max_depth = if recursive { usize::MAX } else { 1 };
    let mut files = Vec::new();
    for entry in WalkDir::new(directory)
        .min_depth(1)
        .max_depth(max_depth)
        .follow_links(false)
        .into_iter()
        .filter_entry(should_visit)
    {
        let entry = entry.map_err(io::Error::other)?;
        if entry.file_type().is_file() {
            files.push(entry.into_path());
        }
    }
    files.sort();
    Ok(files)
}

fn should_visit(entry: &DirEntry) -> bool {
    if !entry.file_type().is_dir() {
        return true;
    }
    !is_ignored_directory_name(&entry.file_name().to_string_lossy())
}

pub(crate) fn is_in_ignored_directory(root: &Path, path: &Path) -> bool {
    let Some(parent) = path.parent() else {
        return false;
    };
    let Ok(relative_parent) = parent.strip_prefix(root) else {
        return false;
    };
    relative_parent.components().any(|component| {
        component
            .as_os_str()
            .to_str()
            .is_some_and(is_ignored_directory_name)
    })
}

pub(crate) fn is_ignored_directory(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(is_ignored_directory_name)
}

fn is_ignored_directory_name(name: &str) -> bool {
    name.starts_with('.')
        || name.eq_ignore_ascii_case("$RECYCLE.BIN")
        || name.eq_ignore_ascii_case("System Volume Information")
}

pub(crate) fn check_rule_paths(rule: &Rule) -> Result<(), String> {
    let source = fs::metadata(&rule.source_dir).map_err(|error| {
        format!(
            "A pasta monitorada '{}' não está disponível: {error}",
            rule.source_dir.display()
        )
    })?;
    if !source.is_dir() {
        return Err(format!(
            "A pasta monitorada '{}' não é uma pasta.",
            rule.source_dir.display()
        ));
    }

    let destination = destination_root(rule);
    let destination_metadata = fs::metadata(&destination).map_err(|error| {
        format!(
            "A raiz do destino '{}' não está disponível: {error}",
            destination.display()
        )
    })?;
    if !destination_metadata.is_dir() {
        return Err(format!(
            "A raiz do destino '{}' não é uma pasta.",
            destination.display()
        ));
    }
    Ok(())
}

pub(crate) fn destination_root(rule: &Rule) -> PathBuf {
    let fixed = rule
        .destination
        .split_once('{')
        .map_or(rule.destination.as_str(), |(prefix, _)| prefix);
    let fixed_path = PathBuf::from(fixed);

    availability_root(&fixed_path)
}

#[cfg(windows)]
fn availability_root(path: &Path) -> PathBuf {
    use std::path::Component;

    let mut root = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => root.push(component.as_os_str()),
            _ => break,
        }
    }
    if root.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        root
    }
}

#[cfg(not(windows))]
fn availability_root(path: &Path) -> PathBuf {
    use std::path::Component;

    let components: Vec<_> = path.components().collect();
    let names: Vec<_> = components
        .iter()
        .filter_map(|component| match component {
            Component::Normal(value) => value.to_str(),
            _ => None,
        })
        .collect();

    if names.first() == Some(&"mnt") {
        return components.iter().take(3).collect();
    }
    if names.first() == Some(&"media") {
        return components.iter().take(4).collect();
    }
    if path.is_absolute() {
        PathBuf::from("/")
    } else {
        PathBuf::from(".")
    }
}

pub(crate) fn is_network_path(path: &Path) -> bool {
    #[cfg(windows)]
    {
        let path = path.to_string_lossy();
        return path.starts_with(r"\\") || path.starts_with("//");
    }

    #[cfg(target_os = "linux")]
    {
        if !(path.starts_with("/mnt") || path.starts_with("/media")) {
            return false;
        }
        let Ok(mounts) = fs::read_to_string("/proc/mounts") else {
            return false;
        };
        mounts.lines().any(|line| {
            let mut fields = line.split_whitespace();
            let _device = fields.next();
            let mount_point = fields.next().map(unescape_mount_path);
            let file_system = fields.next();
            matches!(file_system, Some("cifs" | "nfs" | "nfs4"))
                && mount_point
                    .as_deref()
                    .is_some_and(|mount| path.starts_with(mount))
        })
    }

    #[cfg(not(any(windows, target_os = "linux")))]
    false
}

#[cfg(target_os = "linux")]
fn unescape_mount_path(path: &str) -> PathBuf {
    PathBuf::from(
        path.replace("\\040", " ")
            .replace("\\011", "\t")
            .replace("\\134", "\\"),
    )
}

#[cfg(test)]
mod tests {
    use motor::{Action, CheckMode, ConflictPolicy, DateSource, Rule, Uuid};

    use super::*;

    fn rule(destination: &str) -> Rule {
        Rule {
            id: Uuid::new_v4(),
            name: "Teste".to_owned(),
            enabled: true,
            source_dir: PathBuf::from("/entrada"),
            filter: "*".to_owned(),
            destination: destination.to_owned(),
            action: Action::Move,
            date_source: DateSource::Detected,
            check_mode: CheckMode::Instant,
            include_subfolders: false,
            on_conflict: ConflictPolicy::Rename,
            priority: 0,
        }
    }

    #[test]
    fn finds_fixed_destination_root() {
        assert_eq!(
            destination_root(&rule("/saida/{YYYY}/{MM}")),
            Path::new("/")
        );
        assert_eq!(destination_root(&rule("/saida/ano-{YYYY}")), Path::new("/"));
        assert_eq!(destination_root(&rule("/saida/fixa")), Path::new("/"));
        assert_eq!(
            destination_root(&rule("/mnt/usb/Jornais/{YYYY}")),
            Path::new("/mnt/usb")
        );
        assert_eq!(
            destination_root(&rule("/media/ana/USB/Jornais/{YYYY}")),
            Path::new("/media/ana/USB")
        );
    }
}

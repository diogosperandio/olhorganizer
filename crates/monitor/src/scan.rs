//! Folder listing and availability checks.

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

use motor::{Rule, is_ignored_folder, is_temporary};
use walkdir::WalkDir;

/// Regular files directly inside `dir` or, when `recursive`, anywhere below
/// it, minus temporary download names. Symlinks are not followed, and hidden
/// and system folders are not entered. Only an unreadable `dir` is an error;
/// unreadable subfolders are skipped.
pub fn list_files(dir: &Path, recursive: bool) -> io::Result<Vec<PathBuf>> {
    let walker = WalkDir::new(dir)
        .follow_links(false)
        .min_depth(1)
        .max_depth(if recursive { usize::MAX } else { 1 })
        .into_iter()
        .filter_entry(|e| {
            !(e.depth() > 0
                && e.file_type().is_dir()
                && is_ignored_folder(&e.file_name().to_string_lossy()))
        });

    let mut files = Vec::new();
    for entry in walker {
        match entry {
            Ok(e) => {
                if e.file_type().is_file() && !is_temporary(&e.file_name().to_string_lossy()) {
                    files.push(e.into_path());
                }
            }
            Err(e) if e.depth() == 0 => return Err(e.into()),
            Err(e) => tracing::warn!(error = %e, "subfolder skipped"),
        }
    }
    files.sort();
    Ok(files)
}

/// Checks that the source folder and the disk holding the destination are
/// there. The error is a pt-BR reason for pausing the rule.
pub fn check_available(rule: &Rule) -> Result<(), String> {
    if !fs::metadata(&rule.source_dir).is_ok_and(|m| m.is_dir()) {
        return Err(format!(
            "A pasta monitorada {} não está disponível. A regra será retomada automaticamente quando ela voltar.",
            rule.source_dir.display()
        ));
    }
    let root = destination_root(&rule.destination);
    if !root.exists() {
        return Err(format!(
            "O destino {} não está disponível. A regra será retomada automaticamente quando ele voltar.",
            root.display()
        ));
    }
    Ok(())
}

/// The part of the destination that must already exist: the drive or share on
/// Windows, the mount folder for removable and mounted disks on Linux
/// (`/media/<user>/<disk>`, `/run/media/<user>/<disk>`, `/mnt/<disk>`), or `/`.
/// Folders below it are created on demand.
pub fn destination_root(destination: &str) -> PathBuf {
    let fixed = Path::new(destination.split('{').next().unwrap_or_default());
    let mut components = fixed.components().peekable();

    let mut root = PathBuf::new();
    while let Some(c) =
        components.next_if(|c| matches!(c, Component::Prefix(_) | Component::RootDir))
    {
        root.push(c);
    }
    let rest: Vec<String> = components
        .filter_map(|c| match c {
            Component::Normal(name) => Some(name.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    let depth = match rest
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["media", _, _, ..] => 3,
        ["run", "media", _, _, ..] => 4,
        ["mnt", _, ..] => 2,
        _ => 0,
    };
    // Only whole folder names count: in "/mnt/disk{YYYY}" the folder is variable.
    let cut_by_marker = destination.contains('{')
        && !fixed
            .as_os_str()
            .to_string_lossy()
            .ends_with(std::path::is_separator);
    let complete = rest.len() - usize::from(cut_by_marker && !rest.is_empty());
    if depth <= complete {
        root.extend(&rest[..depth]);
    }
    root
}

/// Whether `path` is on a network share, where change notifications are unreliable.
pub fn is_network_path(path: &Path) -> bool {
    if let Some(Component::Prefix(prefix)) = path.components().next() {
        use std::path::Prefix;
        return matches!(prefix.kind(), Prefix::UNC(..) | Prefix::VerbatimUNC(..));
    }
    linux_network_mount(path)
}

#[cfg(target_os = "linux")]
fn linux_network_mount(path: &Path) -> bool {
    const NETWORK_TYPES: &[&str] = &[
        "cifs",
        "smb3",
        "smbfs",
        "nfs",
        "nfs4",
        "fuse.sshfs",
        "fuse.gvfsd-fuse",
    ];
    let Ok(mounts) = fs::read_to_string("/proc/mounts") else {
        return false;
    };
    // The longest mount point containing `path` is the one it lives on.
    mounts
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let _device = fields.next()?;
            let mount_point = unescape_mount(fields.next()?);
            let fs_type = fields.next()?;
            path.starts_with(&mount_point)
                .then(|| (mount_point.as_os_str().len(), fs_type.to_owned()))
        })
        .max_by_key(|(len, _)| *len)
        .is_some_and(|(_, fs_type)| NETWORK_TYPES.contains(&fs_type.as_str()))
}

#[cfg(not(target_os = "linux"))]
fn linux_network_mount(_path: &Path) -> bool {
    false
}

/// /proc/mounts writes spaces and a few other characters as `\ooo` octal escapes.
#[cfg(target_os = "linux")]
fn unescape_mount(field: &str) -> PathBuf {
    let bytes = field.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let octal = bytes.get(i + 1..i + 4).and_then(|d| {
            std::str::from_utf8(d)
                .ok()
                .and_then(|s| u8::from_str_radix(s, 8).ok())
        });
        match (bytes[i], octal) {
            (b'\\', Some(byte)) => {
                out.push(byte);
                i += 4;
            }
            (byte, _) => {
                out.push(byte);
                i += 1;
            }
        }
    }
    PathBuf::from(String::from_utf8_lossy(&out).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn destination_roots() {
        let cases = [
            ("/home/ana/Jornais/{YYYY}/{MM}", "/"),
            ("/media/ana/PENDRIVE/Jornais/{YYYY}", "/media/ana/PENDRIVE"),
            ("/media/ana/PENDRIVE{YYYY}", "/"),
            ("/run/media/ana/HD/{YYYY}", "/run/media/ana/HD"),
            ("/mnt/backup/{YYYY}", "/mnt/backup"),
            ("/mnt/backup", "/mnt/backup"),
        ];
        for (destination, root) in cases {
            assert_eq!(
                destination_root(destination),
                PathBuf::from(root),
                "{destination}"
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn destination_roots() {
        assert_eq!(
            destination_root(r"D:\Jornais\{YYYY}"),
            PathBuf::from(r"D:\")
        );
        assert_eq!(
            destination_root(r"\\servidor\pasta\{YYYY}"),
            PathBuf::from(r"\\servidor\pasta\")
        );
    }

    #[test]
    fn list_files_skips_folders_and_temporary_names() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();
        for name in ["b.pdf", "a.pdf", "c.pdf.part", "~$d.docx"] {
            fs::write(dir.path().join(name), "x").unwrap();
        }
        let names: Vec<_> = list_files(dir.path(), false)
            .unwrap()
            .into_iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["a.pdf", "b.pdf"]);
    }

    #[test]
    fn recursive_listing_skips_hidden_and_system_folders() {
        let dir = tempfile::tempdir().unwrap();
        for (folder, name) in [
            ("", "a.pdf"),
            ("2025/fotos", "b.jpg"),
            ("2025/fotos", "c.jpg.part"),
            (".oculta", "d.pdf"),
            ("x/.git", "e.pdf"),
            ("$RECYCLE.BIN", "f.pdf"),
            ("System Volume Information", "g.pdf"),
        ] {
            fs::create_dir_all(dir.path().join(folder)).unwrap();
            fs::write(dir.path().join(folder).join(name), "x").unwrap();
        }
        let relative: Vec<_> = list_files(dir.path(), true)
            .unwrap()
            .into_iter()
            .map(|p| p.strip_prefix(dir.path()).unwrap().to_path_buf())
            .collect();
        assert_eq!(
            relative,
            [PathBuf::from("2025/fotos/b.jpg"), PathBuf::from("a.pdf")]
        );
    }

    #[cfg(unix)]
    #[test]
    fn recursive_listing_does_not_follow_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("fora.pdf"), "x").unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("atalho")).unwrap();
        std::os::unix::fs::symlink(outside.path().join("fora.pdf"), dir.path().join("link.pdf"))
            .unwrap();
        assert_eq!(list_files(dir.path(), true).unwrap(), Vec::<PathBuf>::new());
    }

    #[test]
    fn missing_folder_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(list_files(&dir.path().join("não existe"), true).is_err());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn unescapes_mount_points() {
        assert_eq!(
            unescape_mount(r"/mnt/meu\040disco"),
            PathBuf::from("/mnt/meu disco")
        );
    }

    #[test]
    fn local_folder_is_not_network() {
        let dir = tempfile::tempdir().unwrap();
        // Temp folders are local on every machine this runs on.
        assert!(!is_network_path(dir.path()));
    }
}

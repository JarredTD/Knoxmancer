//! Safe filesystem operations shared by build and publishing workflows.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use walkdir::WalkDir;

use crate::error::{Error, Result};

/// Result of replacing a destination, including non-fatal cleanup information.
#[derive(Debug)]
pub struct AtomicReplaceResult {
    /// Warning produced when the old backup could not be removed after replacement.
    pub cleanup_warning: Option<String>,
}

/// Recursively copies a directory while rejecting symbolic links.
pub(crate) fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    if !source.is_dir() {
        return Err(Error::project(format!(
            "source directory is missing: {}",
            source.display()
        )));
    }
    fs::create_dir_all(destination).map_err(Error::io)?;
    for entry in WalkDir::new(source).follow_links(false) {
        let entry = entry.map_err(|error| Error::io(std::io::Error::other(error)))?;
        if entry.file_type().is_file() && is_artifact_junk(entry.path()) {
            continue;
        }
        let relative = entry
            .path()
            .strip_prefix(source)
            .expect("walked entry is below source");
        let target = destination.join(relative);
        if entry.file_type().is_symlink() {
            return Err(Error::project(format!(
                "symbolic links are not supported in artifacts: {}",
                entry.path().display()
            )));
        }
        if entry.file_type().is_dir() {
            fs::create_dir_all(&target).map_err(Error::io)?;
        } else if entry.file_type().is_file() {
            copy_file(entry.path(), &target)?;
        }
    }
    Ok(())
}

/// Identifies common repository and operating-system files that do not belong in artifacts.
fn is_artifact_junk(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    name.eq_ignore_ascii_case(".gitkeep")
        || name.eq_ignore_ascii_case(".DS_Store")
        || name.eq_ignore_ascii_case("Thumbs.db")
        || path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("tmp"))
}

/// Copies one file after creating its destination directory.
pub(crate) fn copy_file(source: &Path, destination: &Path) -> Result<()> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(Error::io)?;
    }
    fs::copy(source, destination).map(|_| ()).map_err(|error| {
        Error::io(std::io::Error::new(
            error.kind(),
            format!(
                "could not copy {} to {}: {error}",
                source.display(),
                destination.display()
            ),
        ))
    })
}

/// Copies a directory into a sibling staging path and atomically replaces its destination.
pub(crate) fn replace_with_copy(source: &Path, destination: &Path) -> Result<AtomicReplaceResult> {
    let parent = destination
        .parent()
        .ok_or_else(|| Error::project("copy destination has no parent"))?;
    let id = destination
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| Error::project("copy destination has no valid directory name"))?;
    fs::create_dir_all(parent).map_err(Error::io)?;
    let staging = staging_path(parent, id);
    remove_tree_if_exists(&staging)?;
    let result = copy_tree(source, &staging).and_then(|()| atomic_replace(&staging, destination));
    if result.is_err() {
        let _ = remove_tree_if_exists(&staging);
    }
    result
}

/// Atomically replaces a directory and attempts rollback on failure.
pub(crate) fn atomic_replace(staging: &Path, destination: &Path) -> Result<AtomicReplaceResult> {
    let parent = destination
        .parent()
        .ok_or_else(|| Error::project("artifact destination has no parent"))?;
    fs::create_dir_all(parent).map_err(Error::io)?;
    let backup = parent.join(format!(
        ".{}-backup-{}",
        destination
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("artifact"),
        unique_token()
    ));
    if destination.exists()
        && let Err(error) = fs::rename(destination, &backup)
    {
        return Err(destination_error(
            error,
            destination,
            "could not move the existing directory",
        ));
    }
    if let Err(error) = fs::rename(staging, destination) {
        if backup.exists()
            && !destination.exists()
            && let Err(rollback) = fs::rename(&backup, destination)
        {
            return Err(Error::io(std::io::Error::new(
                error.kind(),
                format!(
                    "could not install {}: {error}; rollback also failed: {rollback}",
                    destination.display()
                ),
            )));
        }
        return Err(destination_error(
            error,
            destination,
            "could not replace the directory",
        ));
    }
    let cleanup_warning = remove_tree_if_exists(&backup).err().map(|error| {
        format!(
            "replacement succeeded, but old backup {} could not be removed: {error}",
            backup.display()
        )
    });
    Ok(AtomicReplaceResult { cleanup_warning })
}

/// Adds destination context and recovery guidance to a filesystem error.
fn destination_error(error: std::io::Error, destination: &Path, action: &str) -> Error {
    let hint = if error.kind() == std::io::ErrorKind::PermissionDenied {
        " Project Zomboid or another program may be using these files; close it and try again."
    } else {
        ""
    };
    Error::io(std::io::Error::new(
        error.kind(),
        format!("{action} {}: {error}.{hint}", destination.display()),
    ))
}

/// Removes a directory tree after making read-only entries writable.
pub(crate) fn remove_tree_if_exists(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    for entry in WalkDir::new(path)
        .contents_first(true)
        .into_iter()
        .flatten()
    {
        if let Ok(metadata) = entry.path().metadata() {
            let mut permissions = metadata.permissions();
            if permissions.readonly() {
                make_writable(&mut permissions);
                let _ = fs::set_permissions(entry.path(), permissions);
            }
        }
    }
    fs::remove_dir_all(path).map_err(Error::io)
}

#[cfg(windows)]
/// Adds write permission using Windows permission semantics.
fn make_writable(permissions: &mut fs::Permissions) {
    permissions.set_readonly(false);
}

#[cfg(unix)]
/// Adds owner-write permission while preserving other Unix mode bits.
fn make_writable(permissions: &mut fs::Permissions) {
    use std::os::unix::fs::PermissionsExt;
    permissions.set_mode(permissions.mode() | 0o200);
}

/// Creates a collision-resistant sibling path for staged replacement.
pub(crate) fn staging_path(parent: &Path, id: &str) -> PathBuf {
    parent.join(format!(".{id}-staging-{}", unique_token()))
}

/// Combines wall-clock nanoseconds and process identity for temporary names.
fn unique_token() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        ^ u128::from(std::process::id())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn reports_copy_and_atomic_replacement_failures() {
        let temporary = tempdir().unwrap();
        let missing = temporary.path().join("missing");
        assert!(copy_tree(&missing, &temporary.path().join("copy")).is_err());
        assert!(copy_file(&missing, &temporary.path().join("file")).is_err());
        assert!(replace_with_copy(&missing, &temporary.path().join("replaced")).is_err());

        let destination = temporary.path().join("artifact");
        fs::create_dir(&destination).unwrap();
        fs::write(destination.join("old.txt"), "old").unwrap();
        let error = atomic_replace(&missing, &destination).unwrap_err();
        assert!(
            error
                .to_string()
                .contains(&destination.display().to_string())
        );
        assert_eq!(
            fs::read_to_string(destination.join("old.txt")).unwrap(),
            "old"
        );

        let denied = destination_error(
            std::io::Error::from(std::io::ErrorKind::PermissionDenied),
            &destination,
            "could not replace the directory",
        );
        assert!(denied.to_string().contains("close it and try again"));
    }

    #[test]
    fn removes_read_only_trees() {
        let temporary = tempdir().unwrap();
        let tree = temporary.path().join("tree");
        fs::create_dir(&tree).unwrap();
        let file = tree.join("readonly.txt");
        fs::write(&file, "data").unwrap();
        let mut permissions = file.metadata().unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&file, permissions).unwrap();
        remove_tree_if_exists(&tree).unwrap();
        assert!(!tree.exists());
        remove_tree_if_exists(&tree).unwrap();
    }

    #[test]
    fn omits_common_junk_from_copied_artifacts() {
        let temporary = tempdir().unwrap();
        let source = temporary.path().join("source");
        let destination = temporary.path().join("destination");
        fs::create_dir(&source).unwrap();
        for name in [".gitkeep", ".DS_Store", "Thumbs.db", "scratch.tmp"] {
            fs::write(source.join(name), "junk").unwrap();
        }
        fs::write(source.join("mod.info"), "kept").unwrap();

        copy_tree(&source, &destination).unwrap();

        assert_eq!(
            fs::read_to_string(destination.join("mod.info")).unwrap(),
            "kept"
        );
        assert_eq!(destination.read_dir().unwrap().count(), 1);
    }
}

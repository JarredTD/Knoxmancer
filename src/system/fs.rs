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

/// Copies one file after creating its destination directory.
pub(crate) fn copy_file(source: &Path, destination: &Path) -> Result<()> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(Error::io)?;
    }
    fs::copy(source, destination).map(|_| ()).map_err(|error| {
        Error::io(std::io::Error::new(
            error.kind(),
            format!("{}: {error}", source.display()),
        ))
    })
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
        return Err(Error::io(std::io::Error::new(
            error.kind(),
            format!(
                "could not move existing directory {}: {error}",
                destination.display()
            ),
        )));
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
        return Err(Error::io(std::io::Error::new(
            error.kind(),
            format!(
                "could not replace directory {}: {error}",
                destination.display()
            ),
        )));
    }
    let cleanup_warning = remove_tree_if_exists(&backup).err().map(|error| {
        format!(
            "replacement succeeded, but old backup {} could not be removed: {error}",
            backup.display()
        )
    });
    Ok(AtomicReplaceResult { cleanup_warning })
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
}

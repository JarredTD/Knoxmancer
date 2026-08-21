//! Safe filesystem operations shared by build and publishing workflows.

use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use walkdir::WalkDir;

use crate::error::{Error, Result};

/// Filesystem mutations used by replacement operations.
trait MutationFs {
    /// Creates a directory and any missing ancestors.
    fn create_dir_all(&self, path: &Path) -> io::Result<()>;
    /// Renames a filesystem entry.
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
    /// Removes a directory tree.
    fn remove_dir_all(&self, path: &Path) -> io::Result<()>;
    #[cfg(windows)]
    /// Removes an empty directory or directory link.
    fn remove_dir(&self, path: &Path) -> io::Result<()>;
    /// Removes a file or file link.
    fn remove_file(&self, path: &Path) -> io::Result<()>;
    /// Replaces filesystem permissions.
    fn set_permissions(&self, path: &Path, permissions: fs::Permissions) -> io::Result<()>;
    /// Creates and durably writes a new file.
    fn write_synced(&self, path: &Path, contents: &[u8]) -> io::Result<()>;
    /// Flushes directory metadata where the platform supports it.
    fn sync_dir(&self, path: &Path) -> io::Result<()>;
}

/// Production filesystem mutation implementation.
struct RealFs;

impl MutationFs for RealFs {
    fn create_dir_all(&self, path: &Path) -> io::Result<()> {
        fs::create_dir_all(path)
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        fs::rename(from, to)
    }

    fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
        fs::remove_dir_all(path)
    }

    #[cfg(windows)]
    fn remove_dir(&self, path: &Path) -> io::Result<()> {
        fs::remove_dir(path)
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        fs::remove_file(path)
    }

    fn set_permissions(&self, path: &Path, permissions: fs::Permissions) -> io::Result<()> {
        fs::set_permissions(path, permissions)
    }

    fn write_synced(&self, path: &Path, contents: &[u8]) -> io::Result<()> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        file.write_all(contents)?;
        file.sync_all()
    }

    fn sync_dir(&self, path: &Path) -> io::Result<()> {
        #[cfg(unix)]
        fs::File::open(path)?.sync_all()?;
        #[cfg(not(unix))]
        let _ = path;
        Ok(())
    }
}

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
        let relative = entry.path().strip_prefix(source).map_err(|error| {
            Error::project(format!(
                "walked path {} escaped source {}: {error}",
                entry.path().display(),
                source.display()
            ))
        })?;
        let target = destination.join(relative);
        if is_link(entry.path())? {
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

/// Atomically writes a file and reports any obsolete-backup cleanup failure.
pub(crate) fn atomic_write(destination: &Path, contents: &[u8]) -> Result<AtomicReplaceResult> {
    atomic_write_with(&RealFs, destination, contents)
}

/// Atomically writes a file using supplied filesystem mutations.
fn atomic_write_with(
    filesystem: &impl MutationFs,
    destination: &Path,
    contents: &[u8],
) -> Result<AtomicReplaceResult> {
    let parent = destination
        .parent()
        .ok_or_else(|| Error::project("file destination has no parent"))?;
    let name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| Error::project("file destination has no valid name"))?;
    if destination.exists() && !destination.is_file() {
        return Err(Error::project(format!(
            "file destination is not a regular file: {}",
            destination.display()
        )));
    }
    filesystem.create_dir_all(parent).map_err(Error::io)?;
    let staging = staging_path(parent, name);
    if let Err(error) = filesystem.write_synced(&staging, contents) {
        return Err(with_file_cleanup(
            Error::io(error),
            filesystem.remove_file(&staging),
            &staging,
        ));
    }
    let backup = parent.join(format!(".{name}-backup-{}", unique_token()));

    if destination.exists()
        && let Err(error) = filesystem.rename(destination, &backup)
    {
        return Err(with_file_cleanup(
            destination_error(&error, destination, "could not move the existing file"),
            filesystem.remove_file(&staging),
            &staging,
        ));
    }
    if let Err(error) = filesystem.rename(&staging, destination) {
        if backup.exists()
            && !destination.exists()
            && let Err(rollback) = filesystem.rename(&backup, destination)
        {
            let failure = Error::io(io::Error::new(
                error.kind(),
                format!(
                    "could not replace file {}: {error}; rollback also failed: {rollback}; previous file preserved at {}",
                    destination.display(),
                    backup.display()
                ),
            ));
            return Err(with_file_cleanup(
                failure,
                filesystem.remove_file(&staging),
                &staging,
            ));
        }
        return Err(with_file_cleanup(
            destination_error(&error, destination, "could not replace the file"),
            filesystem.remove_file(&staging),
            &staging,
        ));
    }
    let mut warnings = Vec::new();
    if let Err(error) = filesystem.sync_dir(parent) {
        warnings.push(format!(
            "replacement succeeded, but directory metadata for {} could not be flushed: {error}",
            parent.display()
        ));
    }
    let backup_removed = if backup.exists() {
        match filesystem.remove_file(&backup) {
            Ok(()) => true,
            Err(error) => {
                warnings.push(format!(
                    "replacement succeeded, but old backup {} could not be removed: {error}",
                    backup.display()
                ));
                false
            }
        }
    } else {
        false
    };
    if backup_removed && let Err(error) = filesystem.sync_dir(parent) {
        warnings.push(format!(
            "replacement succeeded, but cleanup metadata for {} could not be flushed: {error}",
            parent.display()
        ));
    }
    let cleanup_warning = (!warnings.is_empty()).then(|| warnings.join("; "));
    Ok(AtomicReplaceResult { cleanup_warning })
}

/// Preserves the primary failure while reporting an unsuccessful staging cleanup.
fn with_file_cleanup(primary: Error, cleanup: io::Result<()>, staging: &Path) -> Error {
    match cleanup {
        Ok(()) => primary,
        Err(cleanup) if cleanup.kind() == io::ErrorKind::NotFound => primary,
        Err(cleanup) => Error::io(io::Error::new(
            cleanup.kind(),
            format!(
                "{primary}; staging file {} also could not be removed: {cleanup}",
                staging.display()
            ),
        )),
    }
}

/// Copies a directory into a sibling staging path and atomically replaces its destination.
pub(crate) fn replace_with_copy(source: &Path, destination: &Path) -> Result<AtomicReplaceResult> {
    replace_with_copy_with_hint(source, destination, None)
}

/// Copies and replaces a directory with recovery guidance for a locked destination.
pub(crate) fn replace_with_copy_with_lock_hint(
    source: &Path,
    destination: &Path,
    lock_hint: &str,
) -> Result<AtomicReplaceResult> {
    replace_with_copy_with_hint(source, destination, Some(lock_hint))
}

/// Copies and replaces a directory with an optional existing-destination hint.
fn replace_with_copy_with_hint(
    source: &Path,
    destination: &Path,
    lock_hint: Option<&str>,
) -> Result<AtomicReplaceResult> {
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
    let result = copy_tree(source, &staging)
        .and_then(|()| atomic_replace_with_hint(&RealFs, &staging, destination, lock_hint));
    cleanup_staging_on_error(result, &staging)
}

/// Removes a staging directory after a failed operation without hiding cleanup failures.
pub(crate) fn cleanup_staging_on_error<T>(result: Result<T>, staging: &Path) -> Result<T> {
    match result {
        Ok(value) => Ok(value),
        Err(primary) => match remove_tree_if_exists(staging) {
            Ok(()) => Err(primary),
            Err(cleanup) => Err(Error::io(io::Error::other(format!(
                "{primary}; staging directory {} also could not be removed: {cleanup}",
                staging.display()
            )))),
        },
    }
}

/// Atomically replaces a directory and attempts rollback on failure.
pub(crate) fn atomic_replace(staging: &Path, destination: &Path) -> Result<AtomicReplaceResult> {
    atomic_replace_with(&RealFs, staging, destination)
}

/// Replaces a directory using the supplied filesystem mutations.
fn atomic_replace_with(
    filesystem: &impl MutationFs,
    staging: &Path,
    destination: &Path,
) -> Result<AtomicReplaceResult> {
    atomic_replace_with_hint(filesystem, staging, destination, None)
}

/// Replaces a directory with optional guidance when moving the existing destination fails.
fn atomic_replace_with_hint(
    filesystem: &impl MutationFs,
    staging: &Path,
    destination: &Path,
    lock_hint: Option<&str>,
) -> Result<AtomicReplaceResult> {
    let parent = destination
        .parent()
        .ok_or_else(|| Error::project("artifact destination has no parent"))?;
    filesystem.create_dir_all(parent).map_err(Error::io)?;
    let backup = parent.join(format!(
        ".{}-backup-{}",
        destination
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("artifact"),
        unique_token()
    ));
    if destination.exists()
        && let Err(error) = filesystem.rename(destination, &backup)
    {
        return Err(destination_error_with_hint(
            &error,
            destination,
            "could not move the existing directory",
            lock_hint,
        ));
    }
    if let Err(error) = filesystem.rename(staging, destination) {
        if backup.exists() {
            if destination.exists() {
                return Err(Error::io(io::Error::new(
                    error.kind(),
                    format!(
                        "could not install {} because another replacement won: {error}; previous directory preserved at {}",
                        destination.display(),
                        backup.display()
                    ),
                )));
            }
            if let Err(rollback) = filesystem.rename(&backup, destination) {
                return Err(Error::io(io::Error::new(
                    error.kind(),
                    format!(
                        "could not install {}: {error}; rollback also failed: {rollback}",
                        destination.display()
                    ),
                )));
            }
        }
        return Err(destination_error(
            &error,
            destination,
            "could not replace the directory",
        ));
    }
    let mut warnings = Vec::new();
    if let Err(error) = filesystem.sync_dir(parent) {
        warnings.push(format!(
            "replacement succeeded, but directory metadata for {} could not be flushed: {error}",
            parent.display()
        ));
    }
    let backup_removed = if backup.exists() {
        match remove_tree_if_exists_with(filesystem, &backup) {
            Ok(()) => true,
            Err(error) => {
                warnings.push(format!(
                    "replacement succeeded, but old backup {} could not be removed: {error}",
                    backup.display()
                ));
                false
            }
        }
    } else {
        false
    };
    if backup_removed && let Err(error) = filesystem.sync_dir(parent) {
        warnings.push(format!(
            "replacement succeeded, but cleanup metadata for {} could not be flushed: {error}",
            parent.display()
        ));
    }
    let cleanup_warning = (!warnings.is_empty()).then(|| warnings.join("; "));
    Ok(AtomicReplaceResult { cleanup_warning })
}

/// Adds destination context and recovery guidance to a filesystem error.
fn destination_error(error: &std::io::Error, destination: &Path, action: &str) -> Error {
    destination_error_with_hint(error, destination, action, None)
}

/// Adds optional recovery guidance only to permission-denied destination failures.
fn destination_error_with_hint(
    error: &std::io::Error,
    destination: &Path,
    action: &str,
    recovery: Option<&str>,
) -> Error {
    let hint = match (error.kind(), recovery) {
        (std::io::ErrorKind::PermissionDenied, Some(recovery)) => format!(" {recovery}"),
        (std::io::ErrorKind::PermissionDenied, None) => {
            " Project Zomboid or another program may be using these files; close it and try again."
                .to_owned()
        }
        _ => String::new(),
    };
    Error::io(std::io::Error::new(
        error.kind(),
        format!("{action} {}: {error}.{hint}", destination.display()),
    ))
}

/// Removes a directory tree after making read-only entries writable.
pub(crate) fn remove_tree_if_exists(path: &Path) -> Result<()> {
    remove_tree_if_exists_with(&RealFs, path)
}

/// Removes a directory tree using the supplied filesystem mutations.
fn remove_tree_if_exists_with(filesystem: &impl MutationFs, path: &Path) -> Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(Error::io(error)),
    };
    if is_link_type(metadata.file_type()) {
        return remove_link(filesystem, path, &metadata);
    }
    for entry in WalkDir::new(path).contents_first(true) {
        let entry = entry.map_err(|error| Error::io(io::Error::other(error)))?;
        let metadata = fs::symlink_metadata(entry.path()).map_err(Error::io)?;
        if !is_link_type(metadata.file_type()) {
            let mut permissions = metadata.permissions();
            if permissions.readonly() {
                make_writable(&mut permissions);
                filesystem
                    .set_permissions(entry.path(), permissions)
                    .map_err(Error::io)?;
            }
        }
    }
    filesystem.remove_dir_all(path).map_err(Error::io)
}

/// Removes a symbolic link or Windows directory junction without following it.
fn remove_link(filesystem: &impl MutationFs, path: &Path, metadata: &fs::Metadata) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileTypeExt;
        if metadata.file_type().is_symlink_dir() {
            return filesystem.remove_dir(path).map_err(Error::io);
        }
    }
    #[cfg(not(windows))]
    let _ = metadata;
    filesystem.remove_file(path).map_err(Error::io)
}

/// Reports whether a path is a symbolic link or Windows reparse-point link.
pub(crate) fn is_link(path: &Path) -> Result<bool> {
    fs::symlink_metadata(path)
        .map(|metadata| is_link_type(metadata.file_type()))
        .map_err(Error::io)
}

/// Reports whether a file type represents a link that must not be traversed.
fn is_link_type(file_type: fs::FileType) -> bool {
    if file_type.is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileTypeExt;
        file_type.is_symlink_dir() || file_type.is_symlink_file()
    }
    #[cfg(not(windows))]
    false
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

/// Combines wall-clock nanoseconds, process identity, and a counter for temporary names.
fn unique_token() -> String {
    static PROCESS_EPOCH: OnceLock<u128> = OnceLock::new();
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let epoch = *PROCESS_EPOCH.get_or_init(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    });
    let process = std::process::id();
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{epoch:x}-{process:x}-{counter:x}")
}

#[cfg(all(test, windows))]
/// Creates a real Windows directory junction for platform-specific tests.
pub(crate) fn create_test_junction(link: &Path, target: &Path) -> io::Result<()> {
    use std::process::{Command, Stdio};

    let status = Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "mklink failed with status {status}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::collections::VecDeque;
    use tempfile::tempdir;

    enum RenameAction {
        Perform,
        Fail(io::ErrorKind),
        ConcurrentWinner,
    }

    struct FaultFs {
        renames: RefCell<VecDeque<RenameAction>>,
        fail_permissions: Cell<bool>,
        fail_removal: Cell<bool>,
        fail_write: Cell<bool>,
        fail_sync: Cell<bool>,
    }

    impl FaultFs {
        fn with_renames(actions: impl IntoIterator<Item = RenameAction>) -> Self {
            Self {
                renames: RefCell::new(actions.into_iter().collect()),
                fail_permissions: Cell::new(false),
                fail_removal: Cell::new(false),
                fail_write: Cell::new(false),
                fail_sync: Cell::new(false),
            }
        }
    }

    impl MutationFs for FaultFs {
        fn create_dir_all(&self, path: &Path) -> io::Result<()> {
            fs::create_dir_all(path)
        }

        fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
            match self
                .renames
                .borrow_mut()
                .pop_front()
                .unwrap_or(RenameAction::Perform)
            {
                RenameAction::Perform => fs::rename(from, to),
                RenameAction::Fail(kind) => Err(io::Error::from(kind)),
                RenameAction::ConcurrentWinner => {
                    fs::create_dir(to)?;
                    fs::write(to.join("winner.txt"), "winner")?;
                    Err(io::Error::from(io::ErrorKind::AlreadyExists))
                }
            }
        }

        fn remove_dir_all(&self, path: &Path) -> io::Result<()> {
            if self.fail_removal.get() {
                Err(io::Error::from(io::ErrorKind::PermissionDenied))
            } else {
                fs::remove_dir_all(path)
            }
        }

        #[cfg(windows)]
        fn remove_dir(&self, path: &Path) -> io::Result<()> {
            fs::remove_dir(path)
        }

        fn remove_file(&self, path: &Path) -> io::Result<()> {
            if self.fail_removal.get() {
                Err(io::Error::from(io::ErrorKind::PermissionDenied))
            } else {
                fs::remove_file(path)
            }
        }

        fn set_permissions(&self, path: &Path, permissions: fs::Permissions) -> io::Result<()> {
            if self.fail_permissions.get() {
                Err(io::Error::from(io::ErrorKind::PermissionDenied))
            } else {
                fs::set_permissions(path, permissions)
            }
        }

        fn write_synced(&self, path: &Path, contents: &[u8]) -> io::Result<()> {
            if self.fail_write.get() {
                Err(io::Error::from(io::ErrorKind::WriteZero))
            } else {
                RealFs.write_synced(path, contents)
            }
        }

        fn sync_dir(&self, _path: &Path) -> io::Result<()> {
            if self.fail_sync.get() {
                Err(io::Error::from(io::ErrorKind::Other))
            } else {
                Ok(())
            }
        }
    }

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
            &std::io::Error::from(std::io::ErrorKind::PermissionDenied),
            &destination,
            "could not replace the directory",
        );
        assert!(denied.to_string().contains("close it and try again"));
    }

    #[test]
    fn adds_live_install_guidance_only_to_locked_destination_moves() {
        let temporary = tempdir().unwrap();
        let destination = temporary.path().join("installed");
        let staging = temporary.path().join("staging");
        fs::create_dir(&destination).unwrap();
        fs::create_dir(&staging).unwrap();
        let filesystem =
            FaultFs::with_renames([RenameAction::Fail(io::ErrorKind::PermissionDenied)]);

        let error = atomic_replace_with_hint(
            &filesystem,
            &staging,
            &destination,
            Some("retry with `km install --live`"),
        )
        .unwrap_err()
        .to_string();
        assert!(error.contains("km install --live"));

        let unrelated = destination_error_with_hint(
            &io::Error::from(io::ErrorKind::Other),
            &destination,
            "could not replace",
            Some("retry with `km install --live`"),
        )
        .to_string();
        assert!(!unrelated.contains("km install --live"));
    }

    #[test]
    fn reports_staging_directory_cleanup_failures() {
        let temporary = tempdir().unwrap();
        let staging = temporary.path().join("staging-file");
        fs::write(&staging, "not a directory").unwrap();
        let result: Result<()> = Err(Error::project("primary failure"));

        let error = cleanup_staging_on_error(result, &staging).unwrap_err();

        assert!(error.to_string().contains("primary failure"));
        assert!(error.to_string().contains("staging directory"));
    }

    #[test]
    fn replacement_reports_rollback_and_cleanup_failures() {
        let temporary = tempdir().unwrap();
        let destination = temporary.path().join("artifact");
        let staging = temporary.path().join("staging");
        fs::create_dir(&destination).unwrap();
        fs::create_dir(&staging).unwrap();
        fs::write(destination.join("old.txt"), "old").unwrap();
        fs::write(staging.join("new.txt"), "new").unwrap();
        let filesystem = FaultFs::with_renames([
            RenameAction::Perform,
            RenameAction::Fail(io::ErrorKind::PermissionDenied),
            RenameAction::Fail(io::ErrorKind::Other),
        ]);

        let error = atomic_replace_with(&filesystem, &staging, &destination)
            .unwrap_err()
            .to_string();
        assert!(error.contains("rollback also failed"));

        let destination = temporary.path().join("cleanup-artifact");
        let staging = temporary.path().join("cleanup-staging");
        fs::create_dir(&destination).unwrap();
        fs::create_dir(&staging).unwrap();
        fs::write(destination.join("old.txt"), "old").unwrap();
        fs::write(staging.join("new.txt"), "new").unwrap();
        let filesystem = FaultFs::with_renames([RenameAction::Perform, RenameAction::Perform]);
        filesystem.fail_removal.set(true);

        let result = atomic_replace_with(&filesystem, &staging, &destination).unwrap();
        assert!(result.cleanup_warning.unwrap().contains("old backup"));
        assert_eq!(
            fs::read_to_string(destination.join("new.txt")).unwrap(),
            "new"
        );

        let destination = temporary.path().join("durability-artifact");
        let staging = temporary.path().join("durability-staging");
        fs::create_dir(&destination).unwrap();
        fs::create_dir(&staging).unwrap();
        let filesystem = FaultFs::with_renames([RenameAction::Perform, RenameAction::Perform]);
        filesystem.fail_sync.set(true);

        let warning = atomic_replace_with(&filesystem, &staging, &destination)
            .unwrap()
            .cleanup_warning
            .unwrap();
        assert!(warning.contains("directory metadata"));
        assert!(warning.contains("cleanup metadata"));

        let destination = temporary.path().join("move-failure-artifact");
        let staging = temporary.path().join("move-failure-staging");
        fs::create_dir(&destination).unwrap();
        fs::create_dir(&staging).unwrap();
        let filesystem =
            FaultFs::with_renames([RenameAction::Fail(io::ErrorKind::PermissionDenied)]);
        assert!(atomic_replace_with(&filesystem, &staging, &destination).is_err());
    }

    #[test]
    fn concurrent_replacement_never_overwrites_the_winner() {
        let temporary = tempdir().unwrap();
        let destination = temporary.path().join("artifact");
        let staging = temporary.path().join("staging");
        fs::create_dir(&destination).unwrap();
        fs::write(destination.join("previous.txt"), "previous").unwrap();
        fs::create_dir(&staging).unwrap();
        fs::write(staging.join("candidate.txt"), "candidate").unwrap();
        let filesystem =
            FaultFs::with_renames([RenameAction::Perform, RenameAction::ConcurrentWinner]);

        let error = atomic_replace_with(&filesystem, &staging, &destination)
            .unwrap_err()
            .to_string();
        assert!(error.contains("another replacement won"));
        assert_eq!(
            fs::read_to_string(destination.join("winner.txt")).unwrap(),
            "winner"
        );
        assert!(staging.join("candidate.txt").is_file());
        let backup = fs::read_dir(temporary.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| {
                path.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with(".artifact-backup-")
            })
            .unwrap();
        assert_eq!(
            fs::read_to_string(backup.join("previous.txt")).unwrap(),
            "previous"
        );
    }

    #[test]
    fn permission_repair_failures_are_not_ignored() {
        let temporary = tempdir().unwrap();
        let tree = temporary.path().join("tree");
        fs::create_dir(&tree).unwrap();
        let file = tree.join("readonly.txt");
        fs::write(&file, "data").unwrap();
        let mut permissions = file.metadata().unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&file, permissions).unwrap();
        let filesystem = FaultFs::with_renames([]);
        filesystem.fail_permissions.set(true);

        assert!(remove_tree_if_exists_with(&filesystem, &tree).is_err());
        assert!(tree.exists());
        let mut permissions = file.metadata().unwrap().permissions();
        make_writable(&mut permissions);
        fs::set_permissions(file, permissions).unwrap();
    }

    #[test]
    fn fault_filesystem_delegates_successful_mutations() {
        let temporary = tempdir().unwrap();
        let filesystem = FaultFs::with_renames([]);
        let file = temporary.path().join("file");
        fs::write(&file, "data").unwrap();
        let permissions = file.metadata().unwrap().permissions();
        filesystem.set_permissions(&file, permissions).unwrap();
        filesystem.remove_file(&file).unwrap();

        let tree = temporary.path().join("tree");
        fs::create_dir(&tree).unwrap();
        filesystem.remove_dir_all(&tree).unwrap();

        let real_file = temporary.path().join("real-file");
        fs::write(&real_file, "data").unwrap();
        let permissions = real_file.metadata().unwrap().permissions();
        RealFs.set_permissions(&real_file, permissions).unwrap();
        RealFs.remove_file(&real_file).unwrap();

        #[cfg(windows)]
        {
            let directory = temporary.path().join("directory");
            fs::create_dir(&directory).unwrap();
            filesystem.remove_dir(&directory).unwrap();

            let real_directory = temporary.path().join("real-directory");
            fs::create_dir(&real_directory).unwrap();
            RealFs.remove_dir(&real_directory).unwrap();
        }
    }

    #[test]
    fn staging_paths_are_unique() {
        let parent = Path::new("artifacts");
        let paths = (0..1_000)
            .map(|_| staging_path(parent, "example"))
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(paths.len(), 1_000);
    }

    #[test]
    fn atomically_creates_and_replaces_files() {
        let temporary = tempdir().unwrap();
        let destination = temporary.path().join("config.toml");
        assert!(
            atomic_write(&destination, b"first")
                .unwrap()
                .cleanup_warning
                .is_none()
        );
        assert!(
            atomic_write(&destination, b"second")
                .unwrap()
                .cleanup_warning
                .is_none()
        );
        assert_eq!(fs::read(destination).unwrap(), b"second");

        let directory = temporary.path().join("directory");
        fs::create_dir(&directory).unwrap();
        assert!(atomic_write(&directory, b"data").is_err());
        assert!(directory.is_dir());

        assert!(atomic_write(Path::new(""), b"data").is_err());
        let blocked_parent = temporary.path().join("blocked");
        fs::write(&blocked_parent, "file").unwrap();
        assert!(atomic_write(&blocked_parent.join("config.toml"), b"data").is_err());
    }

    #[test]
    fn atomic_file_writes_report_rollback_cleanup_and_durability_failures() {
        let temporary = tempdir().unwrap();
        let destination = temporary.path().join("config.toml");
        fs::write(&destination, "old").unwrap();

        let rollback_failure = FaultFs::with_renames([
            RenameAction::Perform,
            RenameAction::Fail(io::ErrorKind::PermissionDenied),
            RenameAction::Fail(io::ErrorKind::Other),
        ]);
        let error = atomic_write_with(&rollback_failure, &destination, b"new")
            .unwrap_err()
            .to_string();
        assert!(error.contains("rollback also failed"));

        let backup = temporary
            .path()
            .read_dir()
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| path.to_string_lossy().contains("-backup-"))
            .unwrap();
        fs::rename(backup, &destination).unwrap();

        let cleanup_failure =
            FaultFs::with_renames([RenameAction::Fail(io::ErrorKind::PermissionDenied)]);
        cleanup_failure.fail_removal.set(true);
        let error = atomic_write_with(&cleanup_failure, &destination, b"new")
            .unwrap_err()
            .to_string();
        assert!(error.contains("staging file"));

        cleanup_failure.fail_removal.set(false);
        for entry in temporary.path().read_dir().unwrap() {
            let path = entry.unwrap().path();
            if path.to_string_lossy().contains("-staging-") {
                fs::remove_file(path).unwrap();
            }
        }

        let successful_rollback = FaultFs::with_renames([
            RenameAction::Perform,
            RenameAction::Fail(io::ErrorKind::PermissionDenied),
            RenameAction::Perform,
        ]);
        assert!(atomic_write_with(&successful_rollback, &destination, b"new").is_err());
        assert_eq!(fs::read_to_string(&destination).unwrap(), "old");

        let durability_failure =
            FaultFs::with_renames([RenameAction::Perform, RenameAction::Perform]);
        durability_failure.fail_removal.set(true);
        durability_failure.fail_sync.set(true);
        let warning = atomic_write_with(&durability_failure, &destination, b"new")
            .unwrap()
            .cleanup_warning
            .unwrap();
        assert!(warning.contains("directory metadata"));
        assert!(warning.contains("old backup"));

        durability_failure.fail_removal.set(false);
        for entry in temporary.path().read_dir().unwrap() {
            let path = entry.unwrap().path();
            if path.to_string_lossy().contains("-backup-") {
                fs::remove_file(path).unwrap();
            }
        }
        fs::write(&destination, "old").unwrap();
        let cleanup_sync_failure =
            FaultFs::with_renames([RenameAction::Perform, RenameAction::Perform]);
        cleanup_sync_failure.fail_sync.set(true);
        let warning = atomic_write_with(&cleanup_sync_failure, &destination, b"new")
            .unwrap()
            .cleanup_warning
            .unwrap();
        assert!(warning.contains("cleanup metadata"));

        let successful_create = FaultFs::with_renames([]);
        atomic_write_with(
            &successful_create,
            &temporary.path().join("created.toml"),
            b"new",
        )
        .unwrap();

        let write_failure = FaultFs::with_renames([]);
        write_failure.fail_write.set(true);
        assert!(
            atomic_write_with(
                &write_failure,
                &temporary.path().join("failed.toml"),
                b"new"
            )
            .is_err()
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

    #[cfg(windows)]
    #[test]
    fn rejects_and_safely_removes_directory_junctions() {
        let temporary = tempdir().unwrap();
        let outside = temporary.path().join("outside");
        let source = temporary.path().join("source");
        let junction = source.join("linked");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("keep.txt"), "keep").unwrap();
        fs::create_dir(&source).unwrap();
        create_test_junction(&junction, &outside).unwrap();

        let error = copy_tree(&source, &temporary.path().join("copy"))
            .unwrap_err()
            .to_string();
        assert!(error.contains("symbolic links are not supported"));
        remove_tree_if_exists(&junction).unwrap();
        assert!(outside.join("keep.txt").is_file());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_and_safely_removes_directory_symlinks() {
        use std::os::unix::fs::symlink;

        let temporary = tempdir().unwrap();
        let outside = temporary.path().join("outside");
        let source = temporary.path().join("source");
        let link = source.join("linked");
        fs::create_dir(&outside).unwrap();
        fs::write(outside.join("keep.txt"), "keep").unwrap();
        fs::create_dir(&source).unwrap();
        symlink(&outside, &link).unwrap();

        assert!(copy_tree(&source, &temporary.path().join("copy")).is_err());
        remove_tree_if_exists(&link).unwrap();
        assert!(outside.join("keep.txt").is_file());
    }
}

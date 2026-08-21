//! Explicit non-atomic synchronization for a loaded local mod directory.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, BufReader, Read};
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use super::DevelopmentArtifact;
use crate::error::{Error, Result};
use crate::project::read_mod_metadata;
use crate::system::environment::zomboid_root;
use crate::system::fs::is_link;

/// One kind of filesystem change considered by live installation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LiveAction {
    /// Adds a file that was absent from the installation.
    Create,
    /// Replaces a file whose bytes differ from the artifact.
    Update,
    /// Deletes a file absent from the artifact.
    Remove,
    /// Retains an already identical file.
    Unchanged,
    /// Checks final installation contents after mutations.
    Verify,
    /// Removes an obsolete empty directory.
    CleanDirectory,
}

impl LiveAction {
    /// Stable structured-output representation.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Update => "update",
            Self::Remove => "remove",
            Self::Unchanged => "unchanged",
            Self::Verify => "verify",
            Self::CleanDirectory => "clean_directory",
        }
    }
}

/// Outcome of one planned live-install operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LiveStatus {
    /// Mutation and its verification succeeded.
    Applied,
    /// No mutation was needed.
    Unchanged,
    /// Mutation or verification failed.
    Failed,
    /// Mutation was withheld because an earlier write failed.
    Skipped,
}

impl LiveStatus {
    /// Stable structured-output representation.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Applied => "applied",
            Self::Unchanged => "unchanged",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }
}

/// Result of one file or directory operation during live installation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LiveOperation {
    /// Intended filesystem operation.
    pub(crate) action: LiveAction,
    /// Final outcome.
    pub(crate) status: LiveStatus,
    /// Path relative to the installed mod root.
    pub(crate) path: PathBuf,
    /// Failure or skip explanation, when applicable.
    pub(crate) message: Option<String>,
}

/// Completed live synchronization, including partial-failure information.
#[derive(Debug)]
pub(crate) struct LiveInstallResult {
    /// Existing local installation that was synchronized.
    pub(crate) path: PathBuf,
    /// Every planned or postcondition operation.
    pub(crate) operations: Vec<LiveOperation>,
}

impl LiveInstallResult {
    /// Whether every planned mutation and final verification succeeded.
    pub(crate) fn is_complete(&self) -> bool {
        self.operations
            .iter()
            .all(|operation| !matches!(operation.status, LiveStatus::Failed | LiveStatus::Skipped))
    }

    /// Counts operations with one action and status.
    pub(crate) fn count(&self, action: LiveAction, status: LiveStatus) -> usize {
        self.operations
            .iter()
            .filter(|operation| operation.action == action && operation.status == status)
            .count()
    }

    /// Whether an applied mutation affected something other than a Lua file.
    pub(crate) fn has_non_lua_changes(&self) -> bool {
        self.operations.iter().any(|operation| {
            operation.status == LiveStatus::Applied
                && matches!(
                    operation.action,
                    LiveAction::Create | LiveAction::Update | LiveAction::Remove
                )
                && !has_extension(&operation.path, "lua")
        })
    }

    /// Whether applied mutations added or removed Lua source files.
    pub(crate) fn has_lua_topology_changes(&self) -> bool {
        self.operations.iter().any(|operation| {
            operation.status == LiveStatus::Applied
                && matches!(operation.action, LiveAction::Create | LiveAction::Remove)
                && has_extension(&operation.path, "lua")
        })
    }
}

/// Filesystem mutations and verification points used by live synchronization.
trait LiveFs {
    /// Creates missing destination directories and copies one file in place.
    fn copy_file(&self, source: &Path, destination: &Path) -> io::Result<()>;
    /// Removes one installed file.
    fn remove_file(&self, path: &Path) -> io::Result<()>;
    /// Removes one empty installed directory.
    fn remove_dir(&self, path: &Path) -> io::Result<()>;
    /// Compares two regular files byte-for-byte.
    fn files_equal(&self, left: &Path, right: &Path) -> io::Result<bool>;
}

/// Production live-install filesystem implementation.
struct RealLiveFs;

impl LiveFs for RealLiveFs {
    fn copy_file(&self, source: &Path, destination: &Path) -> io::Result<()> {
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(source, destination).map(|_| ())
    }

    fn remove_file(&self, path: &Path) -> io::Result<()> {
        fs::remove_file(path)
    }

    fn remove_dir(&self, path: &Path) -> io::Result<()> {
        fs::remove_dir(path)
    }

    fn files_equal(&self, left: &Path, right: &Path) -> io::Result<bool> {
        files_equal(left, right)
    }
}

/// Synchronizes a built artifact into an existing matching local installation.
pub(crate) fn install_live(
    artifact: &DevelopmentArtifact,
    configured_root: Option<&Path>,
) -> Result<LiveInstallResult> {
    install_live_with(&RealLiveFs, artifact, configured_root)
}

/// Live-installs using injected mutation and verification behavior.
fn install_live_with(
    filesystem: &impl LiveFs,
    artifact: &DevelopmentArtifact,
    configured_root: Option<&Path>,
) -> Result<LiveInstallResult> {
    let root = zomboid_root(configured_root, "mods")?;
    let destination = root.join(&artifact.mod_id);
    validate_destination(artifact, &root, &destination)?;
    let source_files = inventory_files(&artifact.path)?;
    let destination_files = inventory_files(&destination)?;
    let destination_directories = inventory_directories(&destination)?;

    let mut operations = Vec::new();
    let mut writes = Vec::new();
    for (relative, source) in &source_files {
        let installed = destination_files.get(relative);
        match installed {
            Some(installed)
                if filesystem
                    .files_equal(source, installed)
                    .map_err(Error::io)? =>
            {
                operations.push(operation(
                    LiveAction::Unchanged,
                    LiveStatus::Unchanged,
                    relative,
                    None,
                ));
            }
            Some(_) => writes.push((LiveAction::Update, relative, source)),
            None => writes.push((LiveAction::Create, relative, source)),
        }
    }
    let removals = destination_files
        .keys()
        .filter(|relative| !source_files.contains_key(*relative))
        .cloned()
        .collect::<Vec<_>>();

    for (action, relative, source) in writes {
        let installed = destination.join(relative);
        let outcome = filesystem
            .copy_file(source, &installed)
            .and_then(|()| verify_equal(filesystem, source, &installed));
        match outcome {
            Ok(()) => operations.push(operation(action, LiveStatus::Applied, relative, None)),
            Err(error) => operations.push(operation(
                action,
                LiveStatus::Failed,
                relative,
                Some(error.to_string()),
            )),
        }
    }

    let write_failed = operations
        .iter()
        .any(|operation| operation.status == LiveStatus::Failed);
    if write_failed {
        operations.extend(removals.iter().map(|relative| {
            operation(
                LiveAction::Remove,
                LiveStatus::Skipped,
                relative,
                Some("not removed because a file update failed".to_owned()),
            )
        }));
    } else {
        for relative in &removals {
            let installed = destination.join(relative);
            let outcome = filesystem.remove_file(&installed).and_then(|()| {
                if installed.exists() {
                    Err(io::Error::other("file still exists after removal"))
                } else {
                    Ok(())
                }
            });
            match outcome {
                Ok(()) => operations.push(operation(
                    LiveAction::Remove,
                    LiveStatus::Applied,
                    relative,
                    None,
                )),
                Err(error) => operations.push(operation(
                    LiveAction::Remove,
                    LiveStatus::Failed,
                    relative,
                    Some(error.to_string()),
                )),
            }
        }
    }

    if !write_failed {
        clean_empty_directories(
            filesystem,
            &destination,
            &destination_directories,
            &mut operations,
        );
    }
    verify_final_tree(
        filesystem,
        &artifact.path,
        &destination,
        &source_files,
        &mut operations,
    )?;
    Ok(LiveInstallResult {
        path: destination,
        operations,
    })
}

/// Validates identity and confinement before live mode can mutate files.
fn validate_destination(
    artifact: &DevelopmentArtifact,
    root: &Path,
    destination: &Path,
) -> Result<()> {
    if destination.parent() != Some(root) {
        return Err(Error::project(format!(
            "unsafe live-install destination: {}",
            destination.display()
        )));
    }
    if !destination.is_dir() {
        return Err(Error::project(format!(
            "live installation requires an existing local copy at {}; run `km install` first",
            destination.display()
        )));
    }
    if is_link(destination)? {
        return Err(Error::project(format!(
            "live installation does not support a linked mod directory: {}",
            destination.display()
        )));
    }
    let canonical_root = fs::canonicalize(root).map_err(Error::io)?;
    let canonical_destination = fs::canonicalize(destination).map_err(Error::io)?;
    if canonical_destination.parent() != Some(canonical_root.as_path()) {
        return Err(Error::project(format!(
            "live-install destination resolves outside the mods root: {}",
            destination.display()
        )));
    }
    let installed = read_mod_metadata(
        &destination.join(&artifact.build).join("mod.info"),
        &artifact.build,
    )?;
    if installed.id != artifact.mod_id {
        return Err(Error::project(format!(
            "installed mod ID `{}` does not match `{}` at {}",
            installed.id,
            artifact.mod_id,
            destination.display()
        )));
    }
    Ok(())
}

/// Inventories regular files and rejects links or unsupported entries.
fn inventory_files(root: &Path) -> Result<BTreeMap<PathBuf, PathBuf>> {
    let mut files = BTreeMap::new();
    for entry in WalkDir::new(root).follow_links(false) {
        let entry = entry.map_err(|error| Error::io(io::Error::other(error)))?;
        let path = entry.path();
        if is_link(path)? {
            return Err(Error::project(format!(
                "live installation does not support links: {}",
                path.display()
            )));
        }
        if entry.file_type().is_file() {
            let relative = relative_path(root, path)?;
            files.insert(relative, path.to_path_buf());
        } else if !entry.file_type().is_dir() {
            return Err(Error::project(format!(
                "live installation found an unsupported filesystem entry: {}",
                path.display()
            )));
        }
    }
    Ok(files)
}

/// Inventories installed subdirectories deepest-first for safe empty cleanup.
fn inventory_directories(root: &Path) -> Result<Vec<PathBuf>> {
    let mut directories = Vec::new();
    for entry in WalkDir::new(root).min_depth(1).follow_links(false) {
        let entry = entry.map_err(|error| Error::io(io::Error::other(error)))?;
        if entry.file_type().is_dir() {
            directories.push(relative_path(root, entry.path())?);
        }
    }
    directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    Ok(directories)
}

/// Removes now-empty directories without treating non-empty directories as failures.
fn clean_empty_directories(
    filesystem: &impl LiveFs,
    destination: &Path,
    directories: &[PathBuf],
    operations: &mut Vec<LiveOperation>,
) {
    for relative in directories {
        let path = destination.join(relative);
        match filesystem.remove_dir(&path) {
            Ok(()) => operations.push(operation(
                LiveAction::CleanDirectory,
                LiveStatus::Applied,
                relative,
                None,
            )),
            Err(error) if error.kind() == io::ErrorKind::DirectoryNotEmpty => {}
            Err(error) => operations.push(operation(
                LiveAction::CleanDirectory,
                LiveStatus::Failed,
                relative,
                Some(error.to_string()),
            )),
        }
    }
}

/// Verifies the final installed file set and contents against the artifact.
fn verify_final_tree(
    filesystem: &impl LiveFs,
    source_root: &Path,
    destination: &Path,
    source_files: &BTreeMap<PathBuf, PathBuf>,
    operations: &mut Vec<LiveOperation>,
) -> Result<()> {
    let installed_files = inventory_files(destination)?;
    let expected = source_files.keys().collect::<BTreeSet<_>>();
    let actual = installed_files.keys().collect::<BTreeSet<_>>();
    for relative in expected.symmetric_difference(&actual) {
        operations.push(operation(
            LiveAction::Verify,
            LiveStatus::Failed,
            relative,
            Some("final installed file set does not match the artifact".to_owned()),
        ));
    }
    for relative in expected.intersection(&actual) {
        let source = source_root.join(relative);
        let installed = destination.join(relative);
        match filesystem.files_equal(&source, &installed) {
            Ok(true) => {}
            Ok(false) => operations.push(operation(
                LiveAction::Verify,
                LiveStatus::Failed,
                relative,
                Some("installed bytes do not match the artifact".to_owned()),
            )),
            Err(error) => operations.push(operation(
                LiveAction::Verify,
                LiveStatus::Failed,
                relative,
                Some(error.to_string()),
            )),
        }
    }
    Ok(())
}

/// Produces one operation record without repeating construction details.
fn operation(
    action: LiveAction,
    status: LiveStatus,
    path: &Path,
    message: Option<String>,
) -> LiveOperation {
    LiveOperation {
        action,
        status,
        path: path.to_path_buf(),
        message,
    }
}

/// Verifies that a just-copied file exactly matches its source.
fn verify_equal(filesystem: &impl LiveFs, source: &Path, installed: &Path) -> io::Result<()> {
    if filesystem.files_equal(source, installed)? {
        Ok(())
    } else {
        Err(io::Error::other("copied bytes failed verification"))
    }
}

/// Compares two files using bounded memory.
fn files_equal(left: &Path, right: &Path) -> io::Result<bool> {
    if fs::metadata(left)?.len() != fs::metadata(right)?.len() {
        return Ok(false);
    }
    let mut left = BufReader::new(fs::File::open(left)?);
    let mut right = BufReader::new(fs::File::open(right)?);
    let mut left_buffer = [0_u8; 8192];
    let mut right_buffer = [0_u8; 8192];
    loop {
        let left_read = left.read(&mut left_buffer)?;
        let right_read = right.read(&mut right_buffer)?;
        if left_read != right_read || left_buffer[..left_read] != right_buffer[..right_read] {
            return Ok(false);
        }
        if left_read == 0 {
            return Ok(true);
        }
    }
}

/// Returns a path proven relative to its walked root.
fn relative_path(root: &Path, path: &Path) -> Result<PathBuf> {
    path.strip_prefix(root)
        .map(Path::to_path_buf)
        .map_err(|error| {
            Error::project(format!(
                "walked path {} escaped {}: {error}",
                path.display(),
                root.display()
            ))
        })
}

/// Compares a path extension without platform-specific case sensitivity.
fn has_extension(path: &Path, expected: &str) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case(expected))
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::ffi::OsStr;

    use tempfile::tempdir;

    use super::*;

    /// Mutation adapter that can fail or corrupt one selected operation.
    struct FaultLiveFs<'a> {
        fail_copy: Option<&'a OsStr>,
        fail_remove: Option<&'a OsStr>,
        preserve_remove: Option<&'a OsStr>,
        fail_directory: Option<&'a OsStr>,
        mismatch: Option<&'a OsStr>,
        comparison_error: Option<&'a OsStr>,
        copy_count: Cell<usize>,
    }

    impl<'a> FaultLiveFs<'a> {
        /// Creates a fault adapter with no failures selected.
        fn new() -> Self {
            Self {
                fail_copy: None,
                fail_remove: None,
                preserve_remove: None,
                fail_directory: None,
                mismatch: None,
                comparison_error: None,
                copy_count: Cell::new(0),
            }
        }
    }

    impl LiveFs for FaultLiveFs<'_> {
        fn copy_file(&self, source: &Path, destination: &Path) -> io::Result<()> {
            self.copy_count.set(self.copy_count.get() + 1);
            if destination.file_name() == self.fail_copy {
                return Err(io::Error::from(io::ErrorKind::PermissionDenied));
            }
            RealLiveFs.copy_file(source, destination)
        }

        fn remove_file(&self, path: &Path) -> io::Result<()> {
            if path.file_name() == self.fail_remove {
                return Err(io::Error::from(io::ErrorKind::PermissionDenied));
            }
            if path.file_name() == self.preserve_remove {
                return Ok(());
            }
            RealLiveFs.remove_file(path)
        }

        fn remove_dir(&self, path: &Path) -> io::Result<()> {
            if path.file_name() == self.fail_directory {
                return Err(io::Error::from(io::ErrorKind::PermissionDenied));
            }
            RealLiveFs.remove_dir(path)
        }

        fn files_equal(&self, left: &Path, right: &Path) -> io::Result<bool> {
            if right.file_name() == self.comparison_error {
                return Err(io::Error::from(io::ErrorKind::PermissionDenied));
            }
            if right.file_name() == self.mismatch {
                return Ok(false);
            }
            RealLiveFs.files_equal(left, right)
        }
    }

    /// Writes a valid build metadata file under one mod root.
    fn write_metadata(root: &Path, id: &str, version: &str) {
        fs::create_dir_all(root.join("42")).unwrap();
        fs::write(
            root.join("42/mod.info"),
            format!("name=Example\nid={id}\nmodversion={version}\n"),
        )
        .unwrap();
    }

    /// Creates matching artifact and installation roots for a test.
    fn fixture(root: &Path) -> (DevelopmentArtifact, PathBuf, PathBuf) {
        let artifact_root = root.join("artifact/Example");
        let mods_root = root.join("mods");
        let installed_root = mods_root.join("Example");
        write_metadata(&artifact_root, "Example", "1.1.0");
        write_metadata(&installed_root, "Example", "1.0.0");
        let artifact = DevelopmentArtifact {
            path: artifact_root,
            mod_id: "Example".to_owned(),
            version: "1.1.0".to_owned(),
            build: "42".to_owned(),
            warnings: Vec::new(),
        };
        (artifact, mods_root, installed_root)
    }

    #[test]
    fn synchronizes_and_verifies_changed_files() {
        let temporary = tempdir().unwrap();
        let (artifact, mods, installed) = fixture(temporary.path());
        let source_lua = artifact.path.join("42/media/lua/client");
        let installed_lua = installed.join("42/media/lua/client");
        fs::create_dir_all(&source_lua).unwrap();
        fs::create_dir_all(installed_lua.join("obsolete/empty")).unwrap();
        fs::write(source_lua.join("updated.lua"), "return 'new'\n").unwrap();
        fs::write(source_lua.join("created.lua"), "return true\n").unwrap();
        fs::write(artifact.path.join("42/asset.txt"), "new asset\n").unwrap();
        fs::write(installed_lua.join("updated.lua"), "return 'old'\n").unwrap();
        fs::write(installed_lua.join("removed.lua"), "return false\n").unwrap();
        fs::write(installed.join("42/asset.txt"), "old asset\n").unwrap();

        let filesystem = FaultLiveFs::new();
        let result = install_live_with(&filesystem, &artifact, Some(&mods)).unwrap();

        assert!(result.is_complete());
        assert_eq!(result.count(LiveAction::Create, LiveStatus::Applied), 1);
        assert_eq!(result.count(LiveAction::Update, LiveStatus::Applied), 3);
        assert_eq!(result.count(LiveAction::Remove, LiveStatus::Applied), 1);
        assert!(result.has_non_lua_changes());
        assert!(result.has_lua_topology_changes());
        let source_files = inventory_files(&artifact.path).unwrap();
        let installed_files = inventory_files(&installed).unwrap();
        assert_eq!(
            source_files.keys().collect::<Vec<_>>(),
            installed_files.keys().collect::<Vec<_>>()
        );
        for relative in source_files.keys() {
            assert!(files_equal(&artifact.path.join(relative), &installed.join(relative)).unwrap());
        }
        assert!(!installed_lua.join("obsolete").exists());

        let unchanged = install_live_with(&filesystem, &artifact, Some(&mods)).unwrap();
        assert!(unchanged.is_complete());
        assert_eq!(
            unchanged.count(LiveAction::Unchanged, LiveStatus::Unchanged),
            inventory_files(&artifact.path).unwrap().len()
        );
    }

    #[test]
    fn exposes_stable_operation_names_and_change_classification() {
        assert_eq!(LiveAction::Create.as_str(), "create");
        assert_eq!(LiveAction::Update.as_str(), "update");
        assert_eq!(LiveAction::Remove.as_str(), "remove");
        assert_eq!(LiveAction::Unchanged.as_str(), "unchanged");
        assert_eq!(LiveAction::Verify.as_str(), "verify");
        assert_eq!(LiveAction::CleanDirectory.as_str(), "clean_directory");
        assert_eq!(LiveStatus::Applied.as_str(), "applied");
        assert_eq!(LiveStatus::Unchanged.as_str(), "unchanged");
        assert_eq!(LiveStatus::Failed.as_str(), "failed");
        assert_eq!(LiveStatus::Skipped.as_str(), "skipped");

        let unchanged = LiveInstallResult {
            path: PathBuf::from("Example"),
            operations: vec![operation(
                LiveAction::Unchanged,
                LiveStatus::Unchanged,
                Path::new("42/file.lua"),
                None,
            )],
        };
        assert!(!unchanged.has_non_lua_changes());
        assert!(!unchanged.has_lua_topology_changes());
    }

    #[test]
    fn compares_file_contents_exactly() {
        let temporary = tempdir().unwrap();
        let left = temporary.path().join("left");
        let right = temporary.path().join("right");
        fs::write(&left, vec![b'a'; 9000]).unwrap();
        fs::write(&right, vec![b'a'; 9000]).unwrap();
        assert!(files_equal(&left, &right).unwrap());

        fs::write(&right, vec![b'a'; 8999]).unwrap();
        assert!(!files_equal(&left, &right).unwrap());

        let mut different = vec![b'a'; 9000];
        different[8500] = b'b';
        fs::write(&right, different).unwrap();
        assert!(!files_equal(&left, &right).unwrap());
    }

    #[test]
    fn reports_final_file_set_content_and_comparison_failures() {
        let temporary = tempdir().unwrap();
        let source = temporary.path().join("source");
        let installed = temporary.path().join("installed");
        fs::create_dir_all(&source).unwrap();
        fs::create_dir_all(&installed).unwrap();
        fs::write(source.join("missing.lua"), "source").unwrap();
        fs::write(source.join("mismatch.lua"), "source").unwrap();
        fs::write(source.join("unreadable.lua"), "source").unwrap();
        fs::write(installed.join("extra.lua"), "extra").unwrap();
        fs::write(installed.join("mismatch.lua"), "installed").unwrap();
        fs::write(installed.join("unreadable.lua"), "installed").unwrap();
        let source_files = inventory_files(&source).unwrap();
        let mut filesystem = FaultLiveFs::new();
        filesystem.mismatch = Some(OsStr::new("mismatch.lua"));
        filesystem.comparison_error = Some(OsStr::new("unreadable.lua"));
        let mut operations = Vec::new();

        verify_final_tree(
            &filesystem,
            &source,
            &installed,
            &source_files,
            &mut operations,
        )
        .unwrap();

        assert_eq!(
            operations
                .iter()
                .filter(|operation| operation.action == LiveAction::Verify)
                .count(),
            4
        );
        assert!(operations.iter().all(|operation| {
            operation.status == LiveStatus::Failed && operation.message.is_some()
        }));
    }

    #[test]
    fn reports_persisted_removals_and_directory_cleanup_failures() {
        let temporary = tempdir().unwrap();
        let (artifact, mods, installed) = fixture(temporary.path());
        fs::write(installed.join("42/preserved.lua"), "stale").unwrap();
        fs::create_dir_all(installed.join("42/locked-empty")).unwrap();
        let mut filesystem = FaultLiveFs::new();
        filesystem.preserve_remove = Some(OsStr::new("preserved.lua"));
        filesystem.fail_directory = Some(OsStr::new("locked-empty"));

        let result = install_live_with(&filesystem, &artifact, Some(&mods)).unwrap();

        assert!(!result.is_complete());
        assert!(result.operations.iter().any(|operation| {
            operation.path.ends_with("preserved.lua")
                && operation.action == LiveAction::Remove
                && operation.status == LiveStatus::Failed
        }));
        assert!(result.operations.iter().any(|operation| {
            operation.path.ends_with("locked-empty")
                && operation.action == LiveAction::CleanDirectory
                && operation.status == LiveStatus::Failed
        }));
    }

    #[test]
    fn rejects_paths_outside_the_walked_root() {
        let temporary = tempdir().unwrap();
        let error = relative_path(temporary.path(), Path::new("outside"))
            .unwrap_err()
            .to_string();
        assert!(error.contains("escaped"));
    }

    #[test]
    fn reports_copy_failure_and_withholds_removals() {
        let temporary = tempdir().unwrap();
        let (artifact, mods, installed) = fixture(temporary.path());
        fs::write(artifact.path.join("42/locked.lua"), "new").unwrap();
        fs::write(installed.join("42/stale.lua"), "stale").unwrap();
        let mut filesystem = FaultLiveFs::new();
        filesystem.fail_copy = Some(OsStr::new("locked.lua"));

        let result = install_live_with(&filesystem, &artifact, Some(&mods)).unwrap();

        assert!(!result.is_complete());
        assert!(result.operations.iter().any(|operation| {
            operation.path.ends_with("locked.lua") && operation.status == LiveStatus::Failed
        }));
        assert!(result.operations.iter().any(|operation| {
            operation.path.ends_with("stale.lua") && operation.status == LiveStatus::Skipped
        }));
        assert!(installed.join("42/stale.lua").is_file());
    }

    #[test]
    fn reports_removal_failures() {
        let temporary = tempdir().unwrap();
        let (artifact, mods, installed) = fixture(temporary.path());
        fs::write(installed.join("42/stale.lua"), "stale").unwrap();
        let mut filesystem = FaultLiveFs::new();
        filesystem.fail_remove = Some(OsStr::new("stale.lua"));

        let result = install_live_with(&filesystem, &artifact, Some(&mods)).unwrap();

        assert!(!result.is_complete());
        assert!(result.operations.iter().any(|operation| {
            operation.path.ends_with("stale.lua") && operation.status == LiveStatus::Failed
        }));
    }

    #[test]
    fn reports_verification_mismatches() {
        let temporary = tempdir().unwrap();
        let (artifact, mods, _) = fixture(temporary.path());
        fs::write(artifact.path.join("42/mismatch.lua"), "expected").unwrap();
        let mut filesystem = FaultLiveFs::new();
        filesystem.mismatch = Some(OsStr::new("mismatch.lua"));

        let result = install_live_with(&filesystem, &artifact, Some(&mods)).unwrap();

        assert!(!result.is_complete());
        assert!(filesystem.copy_count.get() > 0);
        assert!(result.operations.iter().any(|operation| {
            operation.path.ends_with("mismatch.lua") && operation.status == LiveStatus::Failed
        }));
    }

    #[test]
    fn rejects_missing_mismatched_and_unsafe_installations() {
        let temporary = tempdir().unwrap();
        let (artifact, mods, installed) = fixture(temporary.path());
        fs::remove_dir_all(&installed).unwrap();
        assert!(
            install_live(&artifact, Some(&mods))
                .unwrap_err()
                .to_string()
                .contains("existing")
        );

        write_metadata(&installed, "Other", "1.0.0");
        assert!(
            install_live(&artifact, Some(&mods))
                .unwrap_err()
                .to_string()
                .contains("does not match")
        );

        let unsafe_artifact = DevelopmentArtifact {
            mod_id: "../outside".to_owned(),
            ..artifact
        };
        assert!(
            install_live(&unsafe_artifact, Some(&mods))
                .unwrap_err()
                .to_string()
                .contains("unsafe")
        );
    }

    #[cfg(unix)]
    #[test]
    fn rejects_links_inside_the_installation() {
        use std::os::unix::fs::symlink;

        let temporary = tempdir().unwrap();
        let (artifact, mods, installed) = fixture(temporary.path());
        let outside = temporary.path().join("outside");
        fs::write(&outside, "outside").unwrap();
        symlink(&outside, installed.join("42/linked.lua")).unwrap();

        assert!(install_live(&artifact, Some(&mods)).is_err());
        assert_eq!(fs::read_to_string(outside).unwrap(), "outside");
    }

    #[cfg(windows)]
    #[test]
    fn rejects_junctions_inside_the_installation() {
        let temporary = tempdir().unwrap();
        let (artifact, mods, installed) = fixture(temporary.path());
        let outside = temporary.path().join("outside");
        fs::create_dir(&outside).unwrap();
        crate::system::fs::create_test_junction(&installed.join("linked"), &outside).unwrap();

        assert!(install_live(&artifact, Some(&mods)).is_err());
        assert!(outside.is_dir());
    }
}

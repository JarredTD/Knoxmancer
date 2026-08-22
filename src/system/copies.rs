//! Discovery of installed Project Zomboid mod copies.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::system::fs::{is_link, remove_tree_if_exists};

/// Steam application identifier for Project Zomboid.
const PROJECT_ZOMBOID_APP_ID: &str = "108600";

/// A location from which Project Zomboid or its Workshop uploader can read a mod.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CopySource {
    /// User-local playable mod installation.
    Local,
    /// Project staged for the in-game Workshop uploader.
    Staging,
    /// Steam-managed Workshop subscription.
    Steam,
}

impl CopySource {
    /// Stable structured-output value.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Staging => "staging",
            Self::Steam => "steam",
        }
    }

    /// Human-readable source label.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Local => "Local installation",
            Self::Staging => "Workshop staging",
            Self::Steam => "Steam subscription",
        }
    }
}

/// One discovered copy matching the current project's mod identifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InstalledCopy {
    /// Kind of installation containing the copy.
    pub(crate) source: CopySource,
    /// Version declared by the copy, if present.
    pub(crate) version: Option<String>,
    /// Root directory of the matching mod.
    pub(crate) path: PathBuf,
}

impl InstalledCopy {
    /// Whether this copy declares the expected project version.
    pub(crate) fn is_current(&self, version: &str) -> bool {
        self.version.as_deref() == Some(version)
    }
}

/// Discovers matching local, staging, and Steam Workshop copies.
pub(crate) fn discover(
    mod_id: &str,
    build: &str,
    mods_root: &Path,
    workshop_root: &Path,
    configured_steam_root: Option<&Path>,
) -> Result<Vec<InstalledCopy>> {
    let mut copies = Vec::new();
    scan_mods_root(mods_root, CopySource::Local, mod_id, build, &mut copies)?;
    scan_staging(workshop_root, mod_id, build, &mut copies)?;
    for library in steam_libraries(configured_steam_root)? {
        scan_steam_library(&library, mod_id, build, &mut copies)?;
    }
    copies.sort_by(|left, right| {
        left.source
            .as_str()
            .cmp(right.source.as_str())
            .then_with(|| left.path.cmp(&right.path))
    });
    copies.dedup_by(|left, right| left.path == right.path);
    Ok(copies)
}

/// Returns whether mod resolution can select an unexpected or ambiguous copy.
pub(crate) fn has_resolution_conflict(copies: &[InstalledCopy], version: &str) -> bool {
    copies.len() > 1 || copies.iter().any(|copy| !copy.is_current(version))
}

/// Removes every staged mod directory whose validated ID matches this project.
pub(crate) fn remove_staging_copies(
    mod_id: &str,
    build: &str,
    workshop_root: &Path,
) -> Result<Vec<PathBuf>> {
    let mut copies = Vec::new();
    scan_staging(workshop_root, mod_id, build, &mut copies)?;
    let paths = copies.into_iter().map(|copy| copy.path).collect::<Vec<_>>();
    validate_staging_removals(workshop_root, &paths)?;
    for path in &paths {
        remove_tree_if_exists(path).map_err(|error| {
            Error::io(std::io::Error::other(format!(
                "local installation succeeded, but failed to remove Workshop staging copy {}: {error}",
                path.display()
            )))
        })?;
    }
    Ok(paths)
}

/// Proves every staged removal stays below the configured Workshop root.
fn validate_staging_removals(workshop_root: &Path, paths: &[PathBuf]) -> Result<()> {
    if paths.is_empty() {
        return Ok(());
    }
    let canonical_root = fs::canonicalize(workshop_root).map_err(Error::io)?;
    for path in paths {
        let relative = path.strip_prefix(workshop_root).map_err(|_| {
            Error::project(format!(
                "unsafe Workshop staging cleanup path: {}",
                path.display()
            ))
        })?;
        if relative.components().count() != 4 || is_link(path)? {
            return Err(Error::project(format!(
                "unsafe Workshop staging cleanup path: {}",
                path.display()
            )));
        }
        let canonical = fs::canonicalize(path).map_err(Error::io)?;
        if !canonical.starts_with(&canonical_root) {
            return Err(Error::project(format!(
                "Workshop staging copy resolves outside its configured root: {}",
                path.display()
            )));
        }
    }
    Ok(())
}

/// Scans one conventional directory containing mod directories.
fn scan_mods_root(
    root: &Path,
    source: CopySource,
    mod_id: &str,
    build: &str,
    copies: &mut Vec<InstalledCopy>,
) -> Result<()> {
    for directory in child_directories(root)? {
        inspect_mod(&directory, source, mod_id, build, copies)?;
    }
    Ok(())
}

/// Scans every locally staged Workshop project.
fn scan_staging(
    root: &Path,
    mod_id: &str,
    build: &str,
    copies: &mut Vec<InstalledCopy>,
) -> Result<()> {
    for project in child_directories(root)? {
        scan_mods_root(
            &project.join("Contents/mods"),
            CopySource::Staging,
            mod_id,
            build,
            copies,
        )?;
    }
    Ok(())
}

/// Scans every subscribed Project Zomboid Workshop item in one Steam library.
fn scan_steam_library(
    library: &Path,
    mod_id: &str,
    build: &str,
    copies: &mut Vec<InstalledCopy>,
) -> Result<()> {
    let content = library
        .join("steamapps/workshop/content")
        .join(PROJECT_ZOMBOID_APP_ID);
    for item in child_directories(&content)? {
        scan_mods_root(&item.join("mods"), CopySource::Steam, mod_id, build, copies)?;
    }
    Ok(())
}

/// Reads one candidate's identity and records it when the identifier matches.
fn inspect_mod(
    root: &Path,
    source: CopySource,
    mod_id: &str,
    build: &str,
    copies: &mut Vec<InstalledCopy>,
) -> Result<()> {
    let metadata = root.join(build).join("mod.info");
    if !metadata.is_file() {
        return Ok(());
    }
    let bytes = fs::read(&metadata).map_err(Error::io)?;
    let text = String::from_utf8_lossy(&bytes);
    let id = field(&text, "id");
    if id.as_deref() != Some(mod_id) {
        return Ok(());
    }
    copies.push(InstalledCopy {
        source,
        version: field(&text, "modversion"),
        path: root.to_path_buf(),
    });
    Ok(())
}

/// Extracts the first trimmed value of a `mod.info` field.
fn field(source: &str, expected: &str) -> Option<String> {
    source.lines().find_map(|line| {
        let (key, value) = line.split_once('=')?;
        (key.trim() == expected).then(|| value.trim().to_owned())
    })
}

/// Returns child directories while treating an absent container as empty.
fn child_directories(root: &Path) -> Result<Vec<PathBuf>> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut directories = Vec::new();
    for entry in fs::read_dir(root).map_err(Error::io)? {
        let entry = entry.map_err(Error::io)?;
        if entry.file_type().map_err(Error::io)?.is_dir() {
            directories.push(entry.path());
        }
    }
    directories.sort();
    Ok(directories)
}

/// Discovers the primary Steam root and every configured Steam library.
fn steam_libraries(configured: Option<&Path>) -> Result<Vec<PathBuf>> {
    let roots = match configured {
        Some(root) => vec![normalize_steam_root(root)],
        None => conventional_steam_roots(),
    };
    let mut libraries = BTreeSet::new();
    for root in roots {
        libraries.insert(root.clone());
        let manifest = root.join("steamapps/libraryfolders.vdf");
        if !manifest.is_file() {
            continue;
        }
        let source = fs::read_to_string(&manifest).map_err(Error::io)?;
        libraries.extend(vdf_library_paths(&source).into_iter().map(PathBuf::from));
    }
    Ok(libraries.into_iter().collect())
}

/// Accepts either a Steam root or its `steamapps` directory.
fn normalize_steam_root(root: &Path) -> PathBuf {
    if root.file_name().is_some_and(|name| name == "steamapps") {
        root.parent().unwrap_or(root).to_path_buf()
    } else {
        root.to_path_buf()
    }
}

/// Finds platform-conventional Steam installations without requiring configuration.
fn conventional_steam_roots() -> Vec<PathBuf> {
    let mut roots = BTreeSet::new();

    #[cfg(windows)]
    {
        roots.extend(windows_registry_steam_roots());
        for variable in ["ProgramFiles(x86)", "ProgramFiles"] {
            if let Some(path) = std::env::var_os(variable) {
                roots.insert(PathBuf::from(path).join("Steam"));
            }
        }
    }

    #[cfg(target_os = "macos")]
    if let Some(home) = super::environment::home_directory() {
        roots.insert(home.join("Library/Application Support/Steam"));
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    if let Some(home) = super::environment::home_directory() {
        roots.insert(home.join(".local/share/Steam"));
        roots.insert(home.join(".steam/steam"));
    }

    roots.into_iter().collect()
}

/// Reads Steam installation locations registered on Windows.
#[cfg(windows)]
fn windows_registry_steam_roots() -> Vec<PathBuf> {
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY};

    let mut roots = BTreeSet::new();
    let current_user = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok(key) = current_user.open_subkey("Software\\Valve\\Steam")
        && let Ok(path) = key.get_value::<String, _>("SteamPath")
    {
        roots.insert(PathBuf::from(path));
    }
    let local_machine = RegKey::predef(HKEY_LOCAL_MACHINE);
    for (key_path, flags) in [
        ("Software\\Valve\\Steam", KEY_READ | KEY_WOW64_32KEY),
        ("Software\\WOW6432Node\\Valve\\Steam", KEY_READ),
    ] {
        if let Ok(key) = local_machine.open_subkey_with_flags(key_path, flags)
            && let Ok(path) = key.get_value::<String, _>("InstallPath")
        {
            roots.insert(PathBuf::from(path));
        }
    }
    roots.into_iter().collect()
}

/// Extracts `path` values from Valve's line-oriented library manifest.
fn vdf_library_paths(source: &str) -> Vec<String> {
    source
        .lines()
        .filter_map(|line| {
            let values = quoted_values(line);
            values
                .windows(2)
                .find(|pair| pair[0] == "path")
                .map(|pair| pair[1].clone())
        })
        .collect()
}

/// Parses quoted VDF values and their backslash escapes.
fn quoted_values(line: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut escaped = false;
    for character in line.chars() {
        if !quoted {
            if character == '"' {
                quoted = true;
                current.clear();
            }
            continue;
        }
        if escaped {
            current.push(character);
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == '"' {
            values.push(current.clone());
            quoted = false;
        } else {
            current.push(character);
        }
    }
    values
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write_mod(root: &Path, build: &str, id: &str, version: Option<&str>) {
        fs::create_dir_all(root.join(build)).unwrap();
        let version = version
            .map(|value| format!("modversion={value}\n"))
            .unwrap_or_default();
        fs::write(
            root.join(build).join("mod.info"),
            format!("name=Example\nid={id}\n{version}"),
        )
        .unwrap();
    }

    #[test]
    fn discovers_each_copy_source_and_conflicts() {
        let temporary = tempdir().unwrap();
        let mods = temporary.path().join("zomboid/mods");
        let staging = temporary.path().join("zomboid/Workshop");
        let steam = temporary.path().join("Steam");
        write_mod(&mods.join("local-name"), "42", "Example", Some("1.0.0"));
        write_mod(
            &mods.join("second-local-name"),
            "42",
            "Example",
            Some("1.0.0"),
        );
        write_mod(
            &staging.join("project/Contents/mods/staged-name"),
            "42",
            "Example",
            Some("0.9.0"),
        );
        write_mod(
            &steam.join("steamapps/workshop/content/108600/123/mods/subscribed-name"),
            "42",
            "Example",
            None,
        );
        write_mod(&mods.join("other"), "42", "Other", Some("1.0.0"));
        fs::write(mods.join("not-a-mod.txt"), "ignored").unwrap();

        let copies = discover("Example", "42", &mods, &staging, Some(&steam)).unwrap();
        assert_eq!(copies.len(), 4);
        assert!(has_resolution_conflict(&copies, "1.0.0"));
        assert_eq!(
            copies
                .iter()
                .filter(|copy| copy.source == CopySource::Local)
                .count(),
            2
        );
        assert_eq!(copies[2].source, CopySource::Staging);
        assert_eq!(copies[3].source, CopySource::Steam);
    }

    #[test]
    fn removes_only_matching_staged_mod_directories() {
        let temporary = tempdir().unwrap();
        let staging = temporary.path().join("Workshop");
        let first = staging.join("project-a/Contents/mods/first-name");
        let second = staging.join("project-b/Contents/mods/second-name");
        let unrelated = staging.join("project-b/Contents/mods/unrelated");
        write_mod(&first, "42", "Example", Some("0.9.0"));
        write_mod(&second, "42", "Example", Some("1.0.0"));
        write_mod(&unrelated, "42", "Other", Some("1.0.0"));

        let removed = remove_staging_copies("Example", "42", &staging).unwrap();

        assert_eq!(removed, [first.clone(), second.clone()]);
        assert!(!first.exists());
        assert!(!second.exists());
        assert!(unrelated.is_dir());
        assert!(
            remove_staging_copies("Missing", "42", &staging)
                .unwrap()
                .is_empty()
        );
    }

    #[cfg(unix)]
    #[test]
    fn refuses_to_remove_staged_links() {
        use std::os::unix::fs::symlink;

        let temporary = tempdir().unwrap();
        let staging = temporary.path().join("Workshop");
        let container = staging.join("project/Contents/mods");
        let outside = temporary.path().join("outside");
        write_mod(&outside, "42", "Example", Some("1.0.0"));
        fs::create_dir_all(&container).unwrap();
        symlink(&outside, container.join("linked")).unwrap();

        assert!(validate_staging_removals(&staging, &[container.join("linked")]).is_err());
        assert!(outside.is_dir());
    }

    #[test]
    fn parses_steam_libraries_and_ignores_absent_roots() {
        let source = r#"
            "libraryfolders"
            {
                "0" { "path" "C:\\Program Files (x86)\\Steam" }
                "1"
                {
                    "path" "D:\\Games\\Steam"
                }
            }
        "#;
        assert_eq!(
            vdf_library_paths(source),
            ["C:\\Program Files (x86)\\Steam", "D:\\Games\\Steam"]
        );
        let temporary = tempdir().unwrap();
        assert!(
            discover(
                "Missing",
                "42",
                &temporary.path().join("mods"),
                &temporary.path().join("Workshop"),
                Some(&temporary.path().join("Steam")),
            )
            .unwrap()
            .is_empty()
        );

        let steam = temporary.path().join("Steam");
        let extra = temporary.path().join("Extra Library");
        fs::create_dir_all(steam.join("steamapps")).unwrap();
        let encoded_extra = extra.display().to_string().replace('\\', "\\\\");
        fs::write(
            steam.join("steamapps/libraryfolders.vdf"),
            format!(
                "\"libraryfolders\"\n{{\n\t\"1\"\n\t{{\n\t\t\"path\" \"{}\"\n\t}}\n}}\n",
                encoded_extra
            ),
        )
        .unwrap();
        let libraries = steam_libraries(Some(&steam.join("steamapps"))).unwrap();
        assert!(libraries.contains(&steam));
        assert!(libraries.contains(&extra));
    }

    #[test]
    fn treats_one_current_load_candidate_as_unambiguous() {
        let copy = InstalledCopy {
            source: CopySource::Local,
            version: Some("1.0.0".to_owned()),
            path: PathBuf::from("Example"),
        };
        assert!(!has_resolution_conflict(&[copy], "1.0.0"));
        let stale = InstalledCopy {
            source: CopySource::Steam,
            version: Some("0.9.0".to_owned()),
            path: PathBuf::from("Stale"),
        };
        assert!(has_resolution_conflict(&[stale], "1.0.0"));
    }
}

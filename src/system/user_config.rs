//! Machine-specific defaults stored outside project manifests.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::system::fs::atomic_write;

/// User defaults shared by Knoxmancer projects on one machine.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct UserConfig {
    /// Default author used by newly scaffolded projects.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// Default local Project Zomboid mods directory.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mods_root: Option<PathBuf>,
    /// Default Project Zomboid Workshop projects directory.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workshop_root: Option<PathBuf>,
}

/// Loaded user defaults and the file from which they were read.
#[derive(Debug)]
pub(crate) struct LoadedUserConfig {
    /// Platform-appropriate user configuration path.
    pub path: PathBuf,
    /// Whether the configuration file currently exists.
    pub exists: bool,
    /// Parsed machine-specific defaults.
    pub values: UserConfig,
}

/// Loads user defaults from the platform-appropriate configuration file.
pub(crate) fn load() -> Result<LoadedUserConfig> {
    let path = location()?;
    let exists = path.is_file();
    let values = load_from(&path)?;
    Ok(LoadedUserConfig {
        path,
        exists,
        values,
    })
}

/// Persists user defaults and returns any non-fatal backup cleanup warning.
pub(crate) fn save(path: &Path, config: &UserConfig) -> Result<Option<String>> {
    validate(config)?;
    let encoded = toml::to_string_pretty(config).map_err(|error| {
        Error::project(format!("could not serialize user configuration: {error}"))
    })?;
    Ok(atomic_write(path, encoded.as_bytes())?.cleanup_warning)
}

/// Validates values before they affect scaffolding or game-facing paths.
fn validate(config: &UserConfig) -> Result<()> {
    if let Some(author) = &config.author {
        if author.trim().is_empty() {
            return Err(Error::project("default author must not be empty"));
        }
        if author.chars().any(char::is_control) {
            return Err(Error::project(
                "default author must not contain control characters",
            ));
        }
    }
    for (name, path) in [
        ("mods root", config.mods_root.as_deref()),
        ("Workshop root", config.workshop_root.as_deref()),
    ] {
        if let Some(path) = path
            && !path.is_absolute()
        {
            return Err(Error::project(format!(
                "default {name} must be an absolute path"
            )));
        }
    }
    Ok(())
}

/// Reads and validates one explicit user-configuration path.
fn load_from(path: &Path) -> Result<UserConfig> {
    if !path.exists() {
        return Ok(UserConfig::default());
    }
    let source = fs::read_to_string(path).map_err(Error::io)?;
    let config = toml::from_str(&source).map_err(|error| {
        Error::project(format!(
            "invalid user configuration {}: {error}",
            path.display()
        ))
    })?;
    validate(&config)?;
    Ok(config)
}

/// Resolves Knoxmancer's user configuration file for the current platform.
fn location() -> Result<PathBuf> {
    if let Some(path) = env::var_os("KNOXMANCER_CONFIG").map(PathBuf::from) {
        if !path.is_absolute() {
            return Err(Error::project(
                "KNOXMANCER_CONFIG must contain an absolute path",
            ));
        }
        return Ok(path);
    }

    #[cfg(windows)]
    let base = env::var_os("APPDATA").map(PathBuf::from);

    #[cfg(target_os = "macos")]
    let base = crate::system::environment::home_directory()
        .map(|home| home.join("Library/Application Support"));

    #[cfg(all(unix, not(target_os = "macos")))]
    let base = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| crate::system::environment::home_directory().map(|home| home.join(".config")));

    base.map(|base| base.join("knoxmancer/config.toml"))
        .ok_or_else(|| Error::project("user configuration directory is unavailable"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn round_trips_and_replaces_user_configuration() {
        let temporary = tempdir().unwrap();
        let path = temporary.path().join("nested/config.toml");
        assert_eq!(load_from(&path).unwrap(), UserConfig::default());

        let mut config = UserConfig {
            author: Some("Test Author".to_owned()),
            mods_root: Some(temporary.path().join("mods")),
            workshop_root: None,
        };
        assert!(save(&path, &config).unwrap().is_none());
        assert_eq!(load_from(&path).unwrap(), config);

        config.author = None;
        assert!(save(&path, &config).unwrap().is_none());
        assert_eq!(load_from(&path).unwrap(), config);
    }

    #[test]
    fn rejects_invalid_user_configuration() {
        let temporary = tempdir().unwrap();
        let path = temporary.path().join("config.toml");
        for source in [
            "unknown = true\n",
            "author = ''\n",
            "author = \"bad\\nname\"\n",
            "mods_root = 'relative'\n",
            "workshop_root = 'relative'\n",
        ] {
            fs::write(&path, source).unwrap();
            assert!(load_from(&path).is_err());
        }
        assert!(location().unwrap().is_absolute());
    }
}

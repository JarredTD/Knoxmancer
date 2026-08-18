//! Project manifest configuration and discovery.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Filename used to identify a Knoxmancer project root.
pub const MANIFEST_NAME: &str = "knoxmancer.toml";
/// Manifest format understood by this Knoxmancer release.
pub const MANIFEST_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
/// Complete deserialized project manifest.
pub struct Config {
    /// Knoxmancer manifest format version.
    pub manifest_version: u32,
    /// Supported Project Zomboid builds.
    pub project: ProjectConfig,
    /// Project-relative source, public, and output directories.
    pub paths: PathsConfig,
    /// Files added to the downloadable mod package.
    pub package: PackageConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            manifest_version: MANIFEST_VERSION,
            project: ProjectConfig::default(),
            paths: PathsConfig::default(),
            package: PackageConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
/// Project Zomboid build configuration.
pub struct ProjectConfig {
    /// Project Zomboid build directories generated in artifacts.
    pub builds: Vec<String>,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            builds: vec!["42".to_owned()],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
/// Project-relative filesystem layout.
pub struct PathsConfig {
    /// Directory containing game-build source trees.
    pub source: PathBuf,
    /// Directory containing Workshop metadata and assets.
    pub public: PathBuf,
    /// Directory receiving generated artifacts.
    pub output: PathBuf,
}

impl Default for PathsConfig {
    fn default() -> Self {
        Self {
            source: PathBuf::from("src"),
            public: PathBuf::from("public"),
            output: PathBuf::from("dist"),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
/// Files added to downloadable mod packages.
pub struct PackageConfig {
    /// Project-relative files copied into the downloadable mod root.
    pub include: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
/// Loaded manifest paired with its project root.
pub struct Project {
    /// Directory containing `knoxmancer.toml`.
    pub root: PathBuf,
    /// Deserialized manifest configuration.
    pub config: Config,
}

impl Project {
    /// Searches the starting path and its ancestors for a manifest.
    pub fn discover(start: Option<&Path>) -> Result<Self> {
        let start = match start {
            Some(path) => path.to_path_buf(),
            None => std::env::current_dir().map_err(Error::io)?,
        };
        let start = if start.is_absolute() {
            start
        } else {
            std::env::current_dir().map_err(Error::io)?.join(start)
        };
        let start = if start.is_file() {
            start.parent().unwrap_or(&start).to_path_buf()
        } else {
            start
        };

        for directory in start.ancestors() {
            let manifest = directory.join(MANIFEST_NAME);
            if manifest.is_file() {
                return Self::load(directory);
            }
        }
        Err(Error::project(format!(
            "no {MANIFEST_NAME} found above {}",
            start.display()
        )))
    }

    /// Loads a manifest from an explicit project root.
    pub fn load(root: &Path) -> Result<Self> {
        let manifest = root.join(MANIFEST_NAME);
        let source = fs::read_to_string(&manifest).map_err(Error::io)?;
        let config: Config = toml::from_str(&source)
            .map_err(|error| Error::project(format!("invalid {}: {error}", manifest.display())))?;
        if config.manifest_version != MANIFEST_VERSION {
            return Err(Error::project(format!(
                "unsupported manifest version {}; expected {MANIFEST_VERSION}",
                config.manifest_version
            )));
        }
        Ok(Self {
            root: root.to_path_buf(),
            config,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn defaults_are_build_42_and_conventional_paths() {
        let config: Config = toml::from_str("").unwrap();
        assert_eq!(config.manifest_version, MANIFEST_VERSION);
        assert_eq!(config.project.builds, ["42"]);
        assert_eq!(config.paths.source, PathBuf::from("src"));
        assert_eq!(config.paths.output, PathBuf::from("dist"));
    }

    #[test]
    fn discovers_projects_from_nested_files_and_rejects_invalid_manifests() {
        let temporary = tempdir().unwrap();
        let root = temporary.path();
        fs::write(root.join(MANIFEST_NAME), "").unwrap();
        fs::create_dir(root.join("nested")).unwrap();
        let nested_file = root.join("nested/example.lua");
        fs::write(&nested_file, "return true").unwrap();

        let project = Project::discover(Some(&nested_file)).unwrap();
        assert_eq!(project.root, root);

        fs::write(root.join(MANIFEST_NAME), "not valid toml = [").unwrap();
        assert!(Project::load(root).is_err());

        fs::write(root.join(MANIFEST_NAME), "[test]\ncommand = 'cargo test'\n").unwrap();
        assert!(Project::load(root).is_err());

        fs::write(root.join(MANIFEST_NAME), "manifest_version = 2\n").unwrap();
        assert!(Project::load(root).is_err());
    }
}

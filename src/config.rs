use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

pub const MANIFEST_NAME: &str = "knoxmancer.toml";

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Config {
    pub project: ProjectConfig,
    pub paths: PathsConfig,
    pub test: TestConfig,
    pub release: ReleaseConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ProjectConfig {
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
#[serde(default)]
pub struct PathsConfig {
    pub source: PathBuf,
    pub public: PathBuf,
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
#[serde(default)]
pub struct TestConfig {
    pub command: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ReleaseConfig {
    pub include: Vec<PathBuf>,
    pub minify: Option<MinifyConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MinifyConfig {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Project {
    pub root: PathBuf,
    pub config: Config,
}

impl Project {
    pub fn discover(start: Option<&Path>) -> Result<Self> {
        let start = match start {
            Some(path) => path.to_path_buf(),
            None => std::env::current_dir().map_err(Error::io)?,
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

    pub fn load(root: &Path) -> Result<Self> {
        let manifest = root.join(MANIFEST_NAME);
        let source = fs::read_to_string(&manifest).map_err(Error::io)?;
        let config = toml::from_str(&source)
            .map_err(|error| Error::project(format!("invalid {}: {error}", manifest.display())))?;
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
    }
}

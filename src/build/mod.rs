//! Development and release artifact construction and installation.

mod minify;

use crate::error::{Error, Result};
use crate::project::{Project, ProjectLayout, ValidatedProject};
use crate::system::environment::home_directory;
use crate::system::fs::{
    atomic_replace, copy_file, copy_tree, remove_tree_if_exists, staging_path,
};
use minify::minify_lua;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildProfile {
    Development,
    Release,
}

impl BuildProfile {
    pub fn name(self) -> &'static str {
        match self {
            Self::Development => "dev",
            Self::Release => "release",
        }
    }
}

#[derive(Debug)]
pub struct BuildArtifact {
    pub path: PathBuf,
    pub mod_id: String,
    pub profile: BuildProfile,
    pub minified_files: usize,
    pub tool_output: Vec<String>,
    pub warnings: Vec<String>,
}

/// A build artifact proven to use the release profile.
#[derive(Debug)]
pub struct ReleaseArtifact(BuildArtifact);

impl ReleaseArtifact {
    /// Returns the underlying release build artifact.
    pub fn artifact(&self) -> &BuildArtifact {
        &self.0
    }
}

impl TryFrom<BuildArtifact> for ReleaseArtifact {
    type Error = Error;

    fn try_from(artifact: BuildArtifact) -> Result<Self> {
        if artifact.profile != BuildProfile::Release {
            return Err(Error::project(
                "Steam Workshop packaging requires a release artifact",
            ));
        }
        Ok(Self(artifact))
    }
}

#[derive(Debug)]
pub struct CleanResult {
    pub path: PathBuf,
    pub removed: bool,
}

/// Result of installing an artifact locally.
#[derive(Debug)]
pub struct InstallResult {
    pub path: PathBuf,
    pub warnings: Vec<String>,
}

pub fn build(validated: &ValidatedProject<'_>, profile: BuildProfile) -> Result<BuildArtifact> {
    let project = validated.project;
    let metadata = &validated.metadata;
    let output = validated.layout.output_root()?;
    let destination = output.join(profile.name()).join(&metadata.id);
    let staging = staging_path(
        destination.parent().expect("profile directory"),
        &metadata.id,
    );

    remove_tree_if_exists(&staging)?;
    fs::create_dir_all(&staging).map_err(Error::io)?;
    let mut minified_files = 0;
    let mut tool_output = Vec::new();
    let result = (|| {
        copy_mod_source(project, &staging)?;
        copy_public_assets(validated, &staging)?;
        for included in &project.config.release.include {
            let (relative, source) = validated.layout.included(included)?;
            if source.is_file() {
                copy_file(&source, &staging.join(relative))?;
            }
        }
        if profile == BuildProfile::Release
            && let Some(minifier) = &project.config.release.minify
        {
            let result = minify_lua(&staging, minifier)?;
            minified_files = result.files;
            tool_output = result.output;
        }
        atomic_replace(&staging, &destination)
    })();
    if result.is_err() {
        let _ = remove_tree_if_exists(&staging);
    }
    let replacement = result?;
    Ok(BuildArtifact {
        path: destination,
        mod_id: metadata.id.clone(),
        profile,
        minified_files,
        tool_output,
        warnings: replacement.cleanup_warning.into_iter().collect(),
    })
}

pub fn install(artifact: &BuildArtifact, configured_root: Option<&Path>) -> Result<InstallResult> {
    let root = match configured_root {
        Some(path) if path.is_absolute() => path.to_path_buf(),
        Some(path) => std::env::current_dir().map_err(Error::io)?.join(path),
        None => home_directory()
            .ok_or_else(|| Error::project("home directory is unavailable; pass --root"))?
            .join("Zomboid/mods"),
    };
    let destination = root.join(&artifact.mod_id);
    if destination.parent() != Some(root.as_path()) {
        return Err(Error::project(format!(
            "unsafe install destination: {}",
            destination.display()
        )));
    }
    fs::create_dir_all(&root).map_err(Error::io)?;
    let staging = staging_path(&root, &artifact.mod_id);
    remove_tree_if_exists(&staging)?;
    copy_tree(&artifact.path, &staging)?;
    let replacement = atomic_replace(&staging, &destination)?;
    Ok(InstallResult {
        path: destination,
        warnings: replacement.cleanup_warning.into_iter().collect(),
    })
}

pub fn clean(project: &Project) -> Result<CleanResult> {
    let output = ProjectLayout::new(project)?.output_root()?;
    let removed = output.exists();
    if removed {
        remove_tree_if_exists(&output)?;
    }
    Ok(CleanResult {
        path: output,
        removed,
    })
}

fn copy_mod_source(project: &Project, destination: &Path) -> Result<()> {
    let source_root = ProjectLayout::new(project)?.source_root()?;
    for directory in project
        .config
        .project
        .builds
        .iter()
        .map(String::as_str)
        .chain(std::iter::once("common"))
    {
        let source = source_root.join(directory);
        if source.is_dir() {
            copy_tree(&source, &destination.join(directory))?;
        }
    }
    Ok(())
}

fn copy_public_assets(validated: &ValidatedProject<'_>, destination: &Path) -> Result<()> {
    let public = validated.layout.public_root()?;
    for name in ["preview.png", "workshop.txt"] {
        copy_file(&public.join(name), &destination.join(name))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::config::Config;
    use tempfile::tempdir;

    fn project(root: &Path) -> Project {
        Project {
            root: root.to_path_buf(),
            config: Config::default(),
        }
    }

    #[test]
    fn clean_rejects_unsafe_output() {
        let temporary = tempdir().unwrap();
        let mut value = project(temporary.path());
        value.config.paths.output = PathBuf::from("src/output");
        assert!(clean(&value).is_err());
    }
}

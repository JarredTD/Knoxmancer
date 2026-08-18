//! Development and release artifact construction and installation.

use crate::error::{Error, Result};
use crate::project::{Project, ProjectLayout, ValidatedProject};
use crate::system::environment::home_directory;
use crate::system::fs::{
    atomic_replace, copy_file, copy_tree, remove_tree_if_exists, staging_path,
};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Artifact policy selected for a build.
pub enum BuildProfile {
    /// Local development artifact with standard validation.
    Development,
    /// Publishing artifact with release-input validation.
    Release,
}

impl BuildProfile {
    /// Returns the directory name used for this profile.
    pub fn name(self) -> &'static str {
        match self {
            Self::Development => "dev",
            Self::Release => "release",
        }
    }
}

#[derive(Debug)]
/// Completed artifact and its identity metadata.
pub struct BuildArtifact {
    /// Root directory of the generated artifact.
    pub path: PathBuf,
    /// Project Zomboid mod identifier.
    pub mod_id: String,
    /// Policy used to build the artifact.
    pub profile: BuildProfile,
    /// Non-fatal cleanup warnings produced during replacement.
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
/// Result of removing a project's generated output tree.
pub struct CleanResult {
    /// Configured output path inspected by the clean operation.
    pub path: PathBuf,
    /// Whether an existing output tree was removed.
    pub removed: bool,
}

/// Result of installing an artifact locally.
#[derive(Debug)]
pub struct InstallResult {
    /// Final local mod installation directory.
    pub path: PathBuf,
    /// Non-fatal cleanup warnings produced during replacement.
    pub warnings: Vec<String>,
}

/// Builds an isolated artifact from a validated project.
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
    let result = (|| {
        copy_mod_source(project, &staging)?;
        copy_public_assets(validated, &staging)?;
        for included in &project.config.release.include {
            let (relative, source) = validated.layout.included(included)?;
            if source.is_file() {
                copy_file(&source, &staging.join(relative))?;
            }
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
        warnings: replacement.cleanup_warning.into_iter().collect(),
    })
}

/// Installs an artifact into a local Project Zomboid mods root.
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

/// Removes all Knoxmancer-generated artifacts for a project.
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

/// Maps the source-oriented project tree into each configured game build.
fn copy_mod_source(project: &Project, destination: &Path) -> Result<()> {
    let source_root = ProjectLayout::new(project)?.source_root()?;
    for build in &project.config.project.builds {
        let build_root = destination.join(build);
        let media_root = build_root.join("media");
        copy_file(&source_root.join("mod.info"), &build_root.join("mod.info"))?;
        let media = source_root.join("media");
        if media.is_dir() {
            copy_tree(&media, &media_root)?;
        }
        for scope in ["client", "shared", "server"] {
            let source = source_root.join(scope);
            if source.is_dir() {
                copy_tree(&source, &media_root.join("lua").join(scope))?;
            }
        }
    }
    Ok(())
}

/// Copies assets required by build and package workflows.
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

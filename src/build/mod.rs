//! Local development artifact construction and installation.

use crate::error::{Error, Result};
use crate::project::{Project, ProjectLayout, ValidatedProject};
use crate::system::environment::zomboid_root;
use crate::system::fs::{
    atomic_replace, cleanup_staging_on_error, copy_file, copy_tree, remove_tree_if_exists,
    replace_with_copy, staging_path,
};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug)]
/// Completed local development artifact and its identity metadata.
pub struct DevelopmentArtifact {
    /// Root directory of the generated artifact.
    pub path: PathBuf,
    /// Project Zomboid mod identifier.
    pub mod_id: String,
    /// Non-fatal cleanup warnings produced during replacement.
    pub warnings: Vec<String>,
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

/// Builds an isolated local development artifact from a validated project.
pub fn build(validated: &ValidatedProject<'_>) -> Result<DevelopmentArtifact> {
    let metadata = &validated.metadata;
    let output = validated.layout.output_root()?;
    let destination = output.join("dev").join(&metadata.id);
    let Some(parent) = destination.parent() else {
        return Err(Error::project("development output has no parent directory"));
    };
    let staging = staging_path(parent, &metadata.id);

    remove_tree_if_exists(&staging)?;
    fs::create_dir_all(&staging).map_err(Error::io)?;
    let result = (|| {
        assemble_mod(validated, &staging)?;
        atomic_replace(&staging, &destination)
    })();
    let replacement = cleanup_staging_on_error(result, &staging)?;
    Ok(DevelopmentArtifact {
        path: destination,
        mod_id: metadata.id.clone(),
        warnings: replacement.cleanup_warning.into_iter().collect(),
    })
}

/// Installs an artifact into a local Project Zomboid mods root.
pub fn install(
    artifact: &DevelopmentArtifact,
    configured_root: Option<&Path>,
) -> Result<InstallResult> {
    let root = zomboid_root(configured_root, "mods")?;
    let destination = root.join(&artifact.mod_id);
    if destination.parent() != Some(root.as_path()) {
        return Err(Error::project(format!(
            "unsafe install destination: {}",
            destination.display()
        )));
    }
    let replacement = replace_with_copy(&artifact.path, &destination)?;
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

/// Maps the source-oriented project tree into the configured game build.
pub(crate) fn assemble_mod(validated: &ValidatedProject<'_>, destination: &Path) -> Result<()> {
    let project = validated.project;
    let source_root = ProjectLayout::new(project)?.source_root()?;
    let build_root = destination.join(&project.config.project.build);
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
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::config::Config;
    use crate::project::validation::{self, ValidationTarget};
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

    #[test]
    fn install_rejects_an_unsafe_artifact_id() {
        let temporary = tempdir().unwrap();
        let artifact = DevelopmentArtifact {
            path: temporary.path().join("artifact"),
            mod_id: "../outside".to_owned(),
            warnings: Vec::new(),
        };
        assert!(install(&artifact, Some(temporary.path())).is_err());
    }

    #[test]
    fn failed_build_removes_its_staging_tree() {
        let temporary = tempdir().unwrap();
        let value = project(temporary.path());
        fs::create_dir(temporary.path().join("src")).unwrap();
        let metadata = temporary.path().join("src/mod.info");
        fs::write(&metadata, "name=Example\nid=Example\nmodversion=1.0.0\n").unwrap();
        let validated = validation::check(&value, ValidationTarget::Playable).unwrap();
        fs::remove_file(metadata).unwrap();

        assert!(build(&validated).is_err());
        assert_eq!(
            fs::read_dir(temporary.path().join("dist/dev"))
                .unwrap()
                .count(),
            0
        );
    }
}

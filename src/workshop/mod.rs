//! Steam Workshop upload tree construction.

use std::fs;
use std::path::{Path, PathBuf};

mod description;

use description::render;

use crate::build::assemble_mod;
use crate::error::{Error, Result};
use crate::project::ValidatedProject;
use crate::system::environment::zomboid_root;
use crate::system::fs::{
    atomic_replace, copy_file, remove_tree_if_exists, replace_with_copy, staging_path,
};

/// Builds and atomically replaces a Steam Workshop upload tree.
pub(crate) fn package(validated: &ValidatedProject<'_>) -> Result<PackageResult> {
    let project = validated.project;
    let metadata = &validated.metadata;
    let output = validated.layout.output_root()?;
    let destination = output.join("workshop").join(&metadata.id);
    let Some(parent) = destination.parent() else {
        return Err(Error::project("Workshop output has no parent directory"));
    };
    let staging = staging_path(parent, &metadata.id);
    remove_tree_if_exists(&staging)?;
    fs::create_dir_all(&staging).map_err(Error::io)?;

    let result = (|| {
        let mod_root = staging.join("Contents/mods").join(&metadata.id);
        fs::create_dir_all(&mod_root).map_err(Error::io)?;
        assemble_mod(validated, &mod_root)?;
        for included in &project.config.package.include {
            let (relative, source) = validated.layout.included(included)?;
            copy_file(&source, &mod_root.join(relative))?;
        }
        let public = validated.layout.public_root()?;
        copy_file(&public.join("preview.png"), &staging.join("preview.png"))?;
        let Some(workshop) = validated.workshop.as_ref() else {
            return Err(Error::validation("Workshop metadata was not validated"));
        };
        fs::write(staging.join("workshop.txt"), render(project, workshop)?).map_err(Error::io)?;
        atomic_replace(&staging, &destination)
    })();
    if result.is_err() {
        let _ = remove_tree_if_exists(&staging);
    }
    let replacement = result?;
    Ok(PackageResult {
        path: destination,
        mod_id: metadata.id.clone(),
        warnings: replacement.cleanup_warning.into_iter().collect(),
    })
}

/// Atomically stages a Workshop project for Project Zomboid's uploader.
pub(crate) fn stage(
    package: &PackageResult,
    configured_root: Option<&Path>,
) -> Result<StageResult> {
    let root = zomboid_root(configured_root, "Workshop")?;
    let destination = root.join(&package.mod_id);
    if destination.parent() != Some(root.as_path()) {
        return Err(Error::project(format!(
            "unsafe Workshop project destination: {}",
            destination.display()
        )));
    }
    let replacement = replace_with_copy(&package.path, &destination)?;
    Ok(StageResult {
        path: destination,
        warnings: replacement.cleanup_warning.into_iter().collect(),
    })
}

/// Result of assembling a Steam Workshop upload tree.
#[derive(Debug)]
pub(crate) struct PackageResult {
    /// Root of the completed Workshop upload tree.
    pub path: PathBuf,
    /// Project Zomboid identifier used as the uploader directory name.
    mod_id: String,
    /// Non-fatal cleanup warnings produced during replacement.
    pub warnings: Vec<String>,
}

/// Result of staging a package for Project Zomboid's Workshop uploader.
#[derive(Debug)]
pub(crate) struct StageResult {
    /// Final uploader staging directory.
    pub path: PathBuf,
    /// Non-fatal cleanup warnings produced during replacement.
    pub warnings: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::config::Project;
    use crate::project::validation::{self, ValidationTarget};
    use crate::scaffold::{self, NewProjectOptions};
    use tempfile::tempdir;

    #[test]
    fn failed_package_removes_its_staging_tree() {
        let temporary = tempdir().unwrap();
        let root = temporary.path().join("project");
        scaffold::new_project(&NewProjectOptions {
            directory: root.clone(),
            name: None,
            id: None,
            author: Some("Tester".to_owned()),
        })
        .unwrap();
        let project = Project::load(&root).unwrap();
        let playable = validation::check(&project, ValidationTarget::Playable).unwrap();
        assert!(
            package(&playable)
                .unwrap_err()
                .to_string()
                .contains("Workshop metadata was not validated")
        );
        let validated = validation::check(&project, ValidationTarget::Workshop).unwrap();
        fs::remove_file(root.join("public/preview.png")).unwrap();

        assert!(package(&validated).is_err());
        assert_eq!(fs::read_dir(root.join("dist/workshop")).unwrap().count(), 0);
    }

    #[test]
    fn stage_rejects_an_unsafe_package_id() {
        let temporary = tempdir().unwrap();
        let package = PackageResult {
            path: temporary.path().join("package"),
            mod_id: "../outside".to_owned(),
            warnings: Vec::new(),
        };
        assert!(stage(&package, Some(temporary.path())).is_err());
    }
}

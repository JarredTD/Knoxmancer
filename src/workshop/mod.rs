//! Steam Workshop upload tree construction.

use std::fs;
use std::path::{Path, PathBuf};

mod description;

use description::render;

use crate::build::assemble_mod;
use crate::error::{Error, Result};
use crate::project::ValidatedProject;
use crate::system::fs::{
    atomic_replace, copy_file, copy_tree, remove_tree_if_exists, staging_path,
};

/// Builds and atomically replaces a Steam Workshop upload tree.
pub(crate) fn package(validated: &ValidatedProject<'_>) -> Result<PackageResult> {
    let project = validated.project;
    let metadata = &validated.metadata;
    let output = validated.layout.output_root()?;
    let destination = output.join("workshop").join(&metadata.id);
    let staging = staging_path(
        destination.parent().expect("workshop directory"),
        &metadata.id,
    );
    remove_tree_if_exists(&staging)?;
    fs::create_dir_all(&staging).map_err(Error::io)?;

    let result = (|| {
        let mod_root = staging.join("Contents/mods").join(&metadata.id);
        fs::create_dir_all(&mod_root).map_err(Error::io)?;
        assemble_mod(validated, &mod_root)?;
        for included in &project.config.release.include {
            let (relative, source) = validated.layout.included(included)?;
            if source.is_file() {
                let file_name = relative.file_name().ok_or_else(|| {
                    Error::project(format!("invalid included path: {}", relative.display()))
                })?;
                if file_name == "LICENSE" {
                    copy_file(&source, &mod_root.join(file_name))?;
                } else {
                    copy_file(&source, &staging.join(file_name))?;
                }
            }
        }
        let public = validated.layout.public_root()?;
        copy_file(&public.join("preview.png"), &staging.join("preview.png"))?;
        fs::write(staging.join("workshop.txt"), render(project)?).map_err(Error::io)?;
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

/// Atomically stages a Workshop package for Project Zomboid's uploader.
pub(crate) fn stage(
    package: &PackageResult,
    configured_root: Option<&Path>,
) -> Result<StageResult> {
    let root = match configured_root {
        Some(path) if path.is_absolute() => path.to_path_buf(),
        Some(path) => std::env::current_dir().map_err(Error::io)?.join(path),
        None => crate::system::steam::project_zomboid_mods_root()?,
    };
    let destination = root.join(&package.mod_id);
    if destination.parent() != Some(root.as_path()) {
        return Err(Error::project(format!(
            "unsafe Workshop staging destination: {}",
            destination.display()
        )));
    }
    fs::create_dir_all(&root).map_err(Error::io)?;
    let staging = staging_path(&root, &package.mod_id);
    remove_tree_if_exists(&staging)?;
    copy_tree(&package.path, &staging)?;
    let replacement = atomic_replace(&staging, &destination)?;
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

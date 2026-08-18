//! Cross-file project validation and validated-project construction.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use super::WorkshopMetadata;
use super::config::Project;
use super::diagnostic::Diagnostic;
use super::layout::ProjectLayout;
use super::metadata::{ModMetadata, read as read_metadata};
use super::preview;
use super::workshop;
use crate::error::{Error, Result};

/// Steam Workshop's strict preview-file size limit.
const PREVIEW_MAX_BYTES: u64 = 1_000_000;

/// A project whose configured builds and publishing inputs passed validation.
#[derive(Debug)]
pub struct ValidatedProject<'a> {
    /// Source project that passed validation.
    pub project: &'a Project,
    /// Confined filesystem layout derived from the manifest.
    pub layout: ProjectLayout<'a>,
    /// Shared mod identity established from configured builds.
    pub metadata: ModMetadata,
    /// Parsed Workshop metadata for Workshop-targeted validation.
    pub workshop: Option<WorkshopMetadata>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Artifact requirements applied while validating a project.
pub(crate) enum ValidationTarget {
    /// Requirements needed to build and install the playable mod.
    Playable,
    /// Playable requirements plus Workshop metadata and publishing inputs.
    Workshop,
}

/// Validates a project for the requested artifact target.
pub fn check(project: &Project, target: ValidationTarget) -> Result<ValidatedProject<'_>> {
    let layout = ProjectLayout::new(project)?;
    let mut problems = Vec::new();
    let source_root = layout.source_root()?;
    let mut metadata = Vec::new();

    if project.config.project.builds.is_empty() {
        problems.push(Diagnostic::at(
            "project.builds.empty",
            project.root.join("knoxmancer.toml"),
            "project.builds must not be empty",
        ));
    }
    for build in &project.config.project.builds {
        if build != "42" {
            problems.push(Diagnostic::at(
                "project.build.unsupported",
                project.root.join("knoxmancer.toml"),
                format!("unsupported Project Zomboid build: {build}"),
            ));
            continue;
        }
        let path = source_root.join("mod.info");
        match read_metadata(&path, build) {
            Ok(value) => metadata.push(value),
            Err(error) => problems.push(Diagnostic::new("metadata.invalid", error.to_string())),
        }
    }

    if let Some(first) = metadata.first() {
        for value in &metadata[1..] {
            if value.id != first.id {
                problems.push(Diagnostic::new(
                    "metadata.id.mismatch",
                    format!(
                        "{} build metadata uses ID {}, expected {}",
                        value.build, value.id, first.id
                    ),
                ));
            }
            if value.version != first.version {
                problems.push(Diagnostic::new(
                    "metadata.version.mismatch",
                    format!(
                        "{} build metadata uses version {}, expected {}",
                        value.build, value.version, first.version
                    ),
                ));
            }
        }
    }
    validate_source_layout(&source_root, &mut problems);
    let workshop = if target == ValidationTarget::Workshop {
        let workshop = validate_public(project, &mut problems);
        validate_package(project, &mut problems);
        workshop
    } else {
        None
    };

    if !problems.is_empty() {
        return Err(Error::validation_diagnostics(problems));
    }
    let result = metadata
        .into_iter()
        .next()
        .ok_or_else(|| Error::validation("no mod metadata was found"))?;
    Ok(ValidatedProject {
        project,
        layout,
        metadata: result,
        workshop,
    })
}

/// Rejects source paths that collide with Knoxmancer's generated Lua tree.
fn validate_source_layout(source_root: &Path, problems: &mut Vec<Diagnostic>) {
    let reserved = source_root.join("media/lua");
    if reserved.exists() {
        problems.push(Diagnostic::at(
            "source.layout.reserved",
            reserved,
            "place Lua under src/client, src/shared, or src/server",
        ));
    }
}

/// Validates required Workshop metadata and preview assets.
fn validate_public(project: &Project, problems: &mut Vec<Diagnostic>) -> Option<WorkshopMetadata> {
    let public = ProjectLayout::new(project)
        .and_then(ProjectLayout::public_root)
        .expect("project layout was validated before public assets");
    for name in ["description.md", "preview.png", "workshop.txt"] {
        let path = public.join(name);
        if !path.is_file() {
            problems.push(Diagnostic::at(
                "public.file.missing",
                &path,
                "required file is missing",
            ));
        }
    }
    let preview = public.join("preview.png");
    if let Ok(data) = fs::read(&preview) {
        if data.len() as u64 >= PREVIEW_MAX_BYTES {
            problems.push(Diagnostic::at(
                "preview.too_large",
                &preview,
                "preview must be under 1000 KB",
            ));
        }
        match preview::inspect(&data) {
            Ok((width, height)) => {
                if (width, height) != (256, 256) {
                    problems.push(Diagnostic::at(
                        "preview.dimensions.invalid",
                        &preview,
                        format!("preview must be 256x256, found {width}x{height}"),
                    ));
                }
            }
            Err(error) => {
                problems.push(Diagnostic::at(
                    "preview.invalid_png",
                    &preview,
                    format!("preview is not a valid PNG: {error}"),
                ));
            }
        }
    }
    let workshop_path = public.join("workshop.txt");
    if !workshop_path.is_file() {
        return None;
    }
    match workshop::parse(&workshop_path) {
        Ok(metadata) => Some(metadata),
        Err(diagnostics) => {
            problems.extend(diagnostics);
            None
        }
    }
}

/// Validates package includes and their final mod-relative destinations.
fn validate_package(project: &Project, problems: &mut Vec<Diagnostic>) {
    let mut destinations = BTreeMap::new();
    for included in &project.config.package.include {
        let path = ProjectLayout::new(project)
            .and_then(|layout| layout.included(included))
            .map(|(_, source)| source)
            .expect("project layout was validated before package inputs");
        if !path.is_file() {
            problems.push(Diagnostic::at(
                "package.include.missing",
                &path,
                "package file is missing",
            ));
        }
        let destination = included
            .to_string_lossy()
            .replace('\\', "/")
            .to_ascii_lowercase();
        if let Some(previous) = destinations.insert(destination, included.clone()) {
            problems.push(Diagnostic::at(
                "package.include.collision",
                &path,
                format!(
                    "package include {} conflicts with {}",
                    included.display(),
                    previous.display()
                ),
            ));
        }
        if project.config.project.builds.iter().any(|build| {
            included
                .components()
                .next()
                .is_some_and(|component| component.as_os_str() == std::ffi::OsStr::new(build))
        }) {
            problems.push(Diagnostic::at(
                "package.include.build_collision",
                path,
                format!(
                    "package include {} overlaps generated build content",
                    included.display()
                ),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::config::Config;
    use crate::system::environment::home_directory;
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn png(width: u32, height: u32) -> Vec<u8> {
        preview::generate(width, height)
    }

    fn valid_project(root: &Path) -> Project {
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("public")).unwrap();
        fs::write(
            root.join("src/mod.info"),
            "name=Example\nid=Example\nmodversion=1.0.0\n",
        )
        .unwrap();
        fs::write(
            root.join("CHANGELOG.md"),
            "# Changelog\n\n## 1.0.0\n\n- Note\n",
        )
        .unwrap();
        fs::write(root.join("public/description.md"), "Description").unwrap();
        fs::write(root.join("public/workshop.txt"), "{{DESCRIPTION}}").unwrap();
        fs::write(root.join("public/preview.png"), png(256, 256)).unwrap();
        Project {
            root: root.to_path_buf(),
            config: Config::default(),
        }
    }

    #[test]
    fn rejects_empty_and_unsupported_build_sets() {
        let temporary = tempdir().unwrap();
        let mut value = valid_project(temporary.path());
        value.config.project.builds.push("41".to_owned());
        let error = check(&value, ValidationTarget::Playable)
            .unwrap_err()
            .to_string();
        assert!(error.contains("unsupported Project Zomboid build: 41"));

        value.config.project.builds = vec!["../outside".to_owned()];
        let error = check(&value, ValidationTarget::Playable)
            .unwrap_err()
            .to_string();
        assert!(error.contains("unsupported Project Zomboid build: ../outside"));

        value.config.project.builds.clear();
        assert!(check(&value, ValidationTarget::Playable).is_err());
    }

    #[test]
    fn validates_preview_and_package_failure_modes() {
        let temporary = tempdir().unwrap();
        let mut value = valid_project(temporary.path());
        let preview = temporary.path().join("public/preview.png");
        fs::write(&preview, png(128, 256)).unwrap();
        let mut problems = Vec::new();
        validate_public(&value, &mut problems);
        assert!(
            problems
                .iter()
                .any(|problem| problem.to_string().contains("128x256"))
        );

        fs::write(&preview, vec![0; PREVIEW_MAX_BYTES as usize]).unwrap();
        validate_public(&value, &mut problems);
        assert!(
            problems
                .iter()
                .any(|problem| problem.to_string().contains("under 1000 KB"))
        );

        value.config.package.include.push(PathBuf::from("MISSING"));
        validate_package(&value, &mut problems);
        assert!(
            problems
                .iter()
                .any(|problem| problem.to_string().contains("package file is missing"))
        );

        fs::write(temporary.path().join("NOTICE"), "notice").unwrap();
        value
            .config
            .package
            .include
            .extend([PathBuf::from("NOTICE"), PathBuf::from("NOTICE")]);
        fs::create_dir(temporary.path().join("42")).unwrap();
        fs::write(temporary.path().join("42/extra.txt"), "collision").unwrap();
        value
            .config
            .package
            .include
            .push(PathBuf::from("42/extra.txt"));
        validate_package(&value, &mut problems);
        assert!(
            problems
                .iter()
                .any(|problem| problem.code == "package.include.collision")
        );
        assert!(
            problems
                .iter()
                .any(|problem| problem.code == "package.include.build_collision")
        );
    }

    #[test]
    fn validates_project_files_and_environment_helpers() {
        let temporary = tempdir().unwrap();
        let value = valid_project(temporary.path());

        fs::remove_file(temporary.path().join("public/description.md")).unwrap();
        assert!(check(&value, ValidationTarget::Workshop).is_err());
        assert!(home_directory().is_some());
    }

    #[test]
    fn rejects_game_layout_paths_inside_media() {
        let temporary = tempdir().unwrap();
        let value = valid_project(temporary.path());
        fs::create_dir_all(temporary.path().join("src/media/lua/client")).unwrap();
        let error = check(&value, ValidationTarget::Playable)
            .unwrap_err()
            .to_string();
        assert!(error.contains("src/client, src/shared, or src/server"));
    }
}

//! Validated paths derived from a project manifest.

use std::path::{Component, Path, PathBuf};

use crate::config::Project;
use crate::error::{Error, Result};

/// Project paths proven to remain within the project root.
#[derive(Debug, Clone, Copy)]
pub struct ProjectLayout<'a> {
    project: &'a Project,
}

impl<'a> ProjectLayout<'a> {
    /// Validates the configured source, public, output, and included paths.
    pub fn new(project: &'a Project) -> Result<Self> {
        let layout = Self { project };
        let source = layout.source_root()?;
        let public = layout.public_root()?;
        let output = layout.output_root()?;

        if source != project.root
            && (source == public || source.starts_with(&public) || public.starts_with(&source))
        {
            return Err(Error::project(
                "paths.source and paths.public must not overlap",
            ));
        }
        if output == project.root
            || (source != project.root && output.starts_with(&source))
            || output.starts_with(&public)
            || source.starts_with(&output)
            || public.starts_with(&output)
        {
            return Err(Error::project(format!(
                "unsafe output directory: {}",
                output.display()
            )));
        }
        for included in &project.config.release.include {
            Self::relative(included, "release.include", false)?;
        }
        Ok(layout)
    }

    /// Returns the confined source root.
    pub fn source_root(self) -> Result<PathBuf> {
        Ok(self.project.root.join(Self::relative(
            &self.project.config.paths.source,
            "paths.source",
            true,
        )?))
    }

    /// Returns the confined public-assets root.
    pub fn public_root(self) -> Result<PathBuf> {
        Ok(self.project.root.join(Self::relative(
            &self.project.config.paths.public,
            "paths.public",
            false,
        )?))
    }

    /// Returns the confined generated-output root.
    pub fn output_root(self) -> Result<PathBuf> {
        Ok(self.project.root.join(Self::relative(
            &self.project.config.paths.output,
            "paths.output",
            false,
        )?))
    }

    /// Resolves a release include and returns its normalized relative path and source path.
    pub fn included(self, configured: &Path) -> Result<(PathBuf, PathBuf)> {
        let relative = Self::relative(configured, "release.include", false)?;
        let source = self.project.root.join(&relative);
        Ok((relative, source))
    }

    fn relative(path: &Path, name: &str, allow_project_root: bool) -> Result<PathBuf> {
        let mut normalized = PathBuf::new();
        for component in path.components() {
            match component {
                Component::CurDir => {}
                Component::Normal(part) => normalized.push(part),
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    return Err(Error::project(format!(
                        "{name} must be a relative path without parent traversal"
                    )));
                }
            }
        }
        if normalized.as_os_str().is_empty() && !allow_project_root {
            return Err(Error::project(format!("{name} must not be empty")));
        }
        Ok(normalized)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use tempfile::tempdir;

    fn project(root: &Path) -> Project {
        Project {
            root: root.to_path_buf(),
            config: Config::default(),
        }
    }

    #[test]
    fn resolves_conventional_and_flat_layouts() {
        let temporary = tempdir().unwrap();
        let mut value = project(temporary.path());
        let layout = ProjectLayout::new(&value).unwrap();
        assert_eq!(layout.source_root().unwrap(), temporary.path().join("src"));
        assert_eq!(layout.output_root().unwrap(), temporary.path().join("dist"));

        value.config.paths.source = PathBuf::from(".");
        assert_eq!(
            ProjectLayout::new(&value).unwrap().source_root().unwrap(),
            temporary.path()
        );
    }

    #[test]
    fn rejects_escaping_empty_and_overlapping_paths() {
        let temporary = tempdir().unwrap();
        let mut value = project(temporary.path());

        value.config.paths.source = PathBuf::from("../source");
        assert!(ProjectLayout::new(&value).is_err());
        value.config.paths.source = PathBuf::from("src");
        value.config.paths.public = temporary.path().join("public");
        assert!(ProjectLayout::new(&value).is_err());
        value.config.paths.public = PathBuf::from("src/public");
        assert!(ProjectLayout::new(&value).is_err());
        value.config.paths.public = PathBuf::from("public");
        value.config.paths.output = PathBuf::new();
        assert!(ProjectLayout::new(&value).is_err());
        value.config.paths.output = PathBuf::from("src/generated");
        assert!(ProjectLayout::new(&value).is_err());
        value.config.paths.output = PathBuf::from("dist");
        value.config.release.include = vec![PathBuf::from("../LICENSE")];
        assert!(ProjectLayout::new(&value).is_err());
    }
}

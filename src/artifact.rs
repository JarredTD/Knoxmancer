use crate::config::Project;
use crate::environment::home_directory;
use crate::error::{Error, Result};
use crate::filesystem::{
    atomic_replace, copy_file, copy_tree, remove_tree_if_exists, staging_path,
};
use crate::minify::minify_lua;
use crate::validation::ValidatedProject;
use std::fs;
use std::path::{Component, Path, PathBuf};

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
}

#[derive(Debug)]
pub struct CleanResult {
    pub path: PathBuf,
    pub removed: bool,
}

pub fn build(validated: &ValidatedProject<'_>, profile: BuildProfile) -> Result<BuildArtifact> {
    let project = validated.project;
    let metadata = &validated.metadata;
    let output = output_root(project)?;
    let destination = output.join(profile.name()).join(&metadata.id);
    let staging = staging_path(
        destination.parent().expect("profile directory"),
        &metadata.id,
    );

    remove_tree_if_exists(&staging)?;
    fs::create_dir_all(&staging).map_err(Error::io)?;
    let mut minified_files = 0;
    let result = (|| {
        copy_mod_source(project, &staging)?;
        copy_public_assets(project, &staging)?;
        for included in &project.config.release.include {
            let source = project.root.join(included);
            if source.is_file() {
                copy_file(&source, &staging.join(included))?;
            }
        }
        if profile == BuildProfile::Release
            && let Some(minifier) = &project.config.release.minify
        {
            minified_files = minify_lua(&staging, minifier)?;
        }
        atomic_replace(&staging, &destination)
    })();
    if result.is_err() {
        let _ = remove_tree_if_exists(&staging);
    }
    result?;
    Ok(BuildArtifact {
        path: destination,
        mod_id: metadata.id.clone(),
        profile,
        minified_files,
    })
}

pub fn install(artifact: &BuildArtifact, configured_root: Option<&Path>) -> Result<PathBuf> {
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
    atomic_replace(&staging, &destination)?;
    Ok(destination)
}

pub fn clean(project: &Project) -> Result<CleanResult> {
    let output = output_root(project)?;
    let removed = output.exists();
    if removed {
        remove_tree_if_exists(&output)?;
    }
    Ok(CleanResult {
        path: output,
        removed,
    })
}

pub(crate) fn output_root(project: &Project) -> Result<PathBuf> {
    let configured = &project.config.paths.output;
    if configured.is_absolute()
        || configured
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(Error::project(
            "paths.output must be a relative path without parent traversal",
        ));
    }
    let output = project.root.join(configured);
    let source = project.root.join(&project.config.paths.source);
    let public = project.root.join(&project.config.paths.public);
    if output == project.root || output.starts_with(&source) || output.starts_with(&public) {
        return Err(Error::project(format!(
            "unsafe output directory: {}",
            output.display()
        )));
    }
    Ok(output)
}

fn copy_mod_source(project: &Project, destination: &Path) -> Result<()> {
    let source_root = project.root.join(&project.config.paths.source);
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

fn copy_public_assets(project: &Project, destination: &Path) -> Result<()> {
    let public = project.root.join(&project.config.paths.public);
    for name in ["preview.png", "workshop.txt"] {
        copy_file(&public.join(name), &destination.join(name))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::config::MinifyConfig;
    use crate::metadata::ModMetadata;
    use crate::workshop::{
        DESCRIPTION_MAX_BYTES, markdown_to_bbcode, release_history, render as render_workshop,
    };
    use tempfile::tempdir;

    fn project(root: &Path) -> Project {
        Project {
            root: root.to_path_buf(),
            config: Config::default(),
        }
    }

    #[test]
    fn converts_supported_markdown() {
        let rendered = markdown_to_bbcode(
            "## Heading\n\n**bold** and [link](https://example.com)\n\n- one\n- two",
        );
        assert!(rendered.contains("[h2]Heading[/h2]"));
        assert!(rendered.contains("[b]bold[/b]"));
        assert!(rendered.contains("[url=https://example.com]link[/url]"));
        assert!(rendered.contains("[list]\n[*]one\n[*]two\n[/list]"));
    }

    #[test]
    fn requires_current_release_first() {
        assert!(release_history("# Changelog\n\n## 1.0.0\n\n- Initial", "1.0.0").is_ok());
        assert!(release_history("# Changelog\n\n## 0.9.0\n\n- Old", "1.0.0").is_err());
    }

    #[test]
    fn validates_output_paths() {
        let temporary = tempdir().unwrap();
        let mut value = project(temporary.path());
        assert_eq!(output_root(&value).unwrap(), temporary.path().join("dist"));

        value.config.paths.output = PathBuf::from("../outside");
        assert!(output_root(&value).is_err());
        value.config.paths.output = temporary.path().join("absolute");
        assert!(output_root(&value).is_err());
        value.config.paths.output = PathBuf::from("src/generated");
        assert!(output_root(&value).is_err());
        value.config.paths.output = PathBuf::new();
        assert!(output_root(&value).is_err());
    }

    #[test]
    fn reports_copy_and_atomic_replacement_failures() {
        let temporary = tempdir().unwrap();
        let missing = temporary.path().join("missing");
        assert!(copy_tree(&missing, &temporary.path().join("copy")).is_err());
        assert!(copy_file(&missing, &temporary.path().join("file")).is_err());

        let destination = temporary.path().join("artifact");
        fs::create_dir(&destination).unwrap();
        fs::write(destination.join("old.txt"), "old").unwrap();
        assert!(atomic_replace(&missing, &destination).is_err());
        assert_eq!(
            fs::read_to_string(destination.join("old.txt")).unwrap(),
            "old"
        );
    }

    #[test]
    fn removes_read_only_trees() {
        let temporary = tempdir().unwrap();
        let tree = temporary.path().join("tree");
        fs::create_dir(&tree).unwrap();
        let file = tree.join("readonly.txt");
        fs::write(&file, "data").unwrap();
        let mut permissions = file.metadata().unwrap().permissions();
        permissions.set_readonly(true);
        fs::set_permissions(&file, permissions).unwrap();
        remove_tree_if_exists(&tree).unwrap();
        assert!(!tree.exists());
        remove_tree_if_exists(&tree).unwrap();
    }

    #[test]
    fn reports_minifier_process_and_output_failures() {
        let temporary = tempdir().unwrap();
        let lua = temporary.path().join("42/media/lua/client/example.lua");
        fs::create_dir_all(lua.parent().unwrap()).unwrap();
        fs::write(&lua, "return true").unwrap();

        let missing = MinifyConfig {
            command: "knoxmancer-command-that-does-not-exist".to_owned(),
            args: Vec::new(),
        };
        assert!(minify_lua(temporary.path(), &missing).is_err());

        #[cfg(windows)]
        let failed = MinifyConfig {
            command: "cmd".to_owned(),
            args: vec!["/c".to_owned(), "exit".to_owned(), "1".to_owned()],
        };
        #[cfg(unix)]
        let failed = MinifyConfig {
            command: "false".to_owned(),
            args: Vec::new(),
        };
        assert!(minify_lua(temporary.path(), &failed).is_err());

        #[cfg(windows)]
        let no_output = MinifyConfig {
            command: "cmd".to_owned(),
            args: vec![
                "/c".to_owned(),
                "exit".to_owned(),
                "0".to_owned(),
                "{output}".to_owned(),
            ],
        };
        #[cfg(unix)]
        let no_output = MinifyConfig {
            command: "true".to_owned(),
            args: vec!["{output}".to_owned()],
        };
        assert!(minify_lua(temporary.path(), &no_output).is_err());
    }

    #[test]
    fn validates_workshop_templates_and_limits() {
        let temporary = tempdir().unwrap();
        let value = project(temporary.path());
        let public = temporary.path().join("public");
        fs::create_dir(&public).unwrap();
        fs::write(
            temporary.path().join("CHANGELOG.md"),
            "# Changelog\n\n## 1.0.0\n\n- Note\n",
        )
        .unwrap();
        fs::write(public.join("workshop.txt"), "{{DESCRIPTION}}").unwrap();
        let metadata = ModMetadata {
            name: "Example".to_owned(),
            id: "Example".to_owned(),
            version: "1.0.0".to_owned(),
            build: "42".to_owned(),
        };

        fs::write(public.join("description.md"), "plain").unwrap();
        assert!(
            render_workshop(&value, &metadata)
                .unwrap()
                .contains("description=plain")
        );
        fs::write(public.join("description.md"), "{{CHANGELOG}}\ntrailing").unwrap();
        assert!(render_workshop(&value, &metadata).is_err());
        fs::write(
            public.join("description.md"),
            "x".repeat(DESCRIPTION_MAX_BYTES),
        )
        .unwrap();
        assert!(render_workshop(&value, &metadata).is_err());
        fs::write(public.join("description.md"), "plain").unwrap();
        fs::write(public.join("workshop.txt"), "missing marker").unwrap();
        assert!(render_workshop(&value, &metadata).is_err());

        assert!(release_history("## 1.0.0\n\nNo note", "1.0.0").is_err());
        assert_eq!(
            release_history("## 1.0.0\n\n- New\n\n## 0.9.0\n\n- Old", "1.0.0").unwrap(),
            "### 1.0.0\n- New\n\n### 0.9.0\n- Old"
        );
    }

    #[test]
    fn clean_rejects_unsafe_output() {
        let temporary = tempdir().unwrap();
        let mut value = project(temporary.path());
        value.config.paths.output = PathBuf::from("src/output");
        assert!(clean(&value).is_err());
    }
}

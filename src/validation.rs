use std::fs;
use std::path::Path;

use regex::Regex;
use walkdir::WalkDir;

use crate::config::Project;
use crate::environment::command_exists;
use crate::error::{Error, Result};
use crate::metadata::{ModMetadata, read as read_metadata};

const PREVIEW_MAX_BYTES: u64 = 1_000_000;

/// A project whose configured builds and publishing inputs passed validation.
#[derive(Debug)]
pub struct ValidatedProject<'a> {
    pub project: &'a Project,
    pub metadata: ModMetadata,
}

pub fn check(project: &Project, release: bool) -> Result<ValidatedProject<'_>> {
    let mut problems = Vec::new();
    let source_root = project.root.join(&project.config.paths.source);
    let mut metadata = Vec::new();

    if project.config.project.builds.is_empty() {
        problems.push("knoxmancer.toml: project.builds must not be empty".to_owned());
    }
    for build in &project.config.project.builds {
        let path = source_root.join(build).join("mod.info");
        match read_metadata(&path, build) {
            Ok(value) => metadata.push(value),
            Err(error) => problems.push(error.to_string()),
        }
    }

    if let Some(first) = metadata.first() {
        for value in &metadata[1..] {
            if value.id != first.id {
                problems.push(format!(
                    "{} build metadata uses ID {}, expected {}",
                    value.build, value.id, first.id
                ));
            }
            if value.version != first.version {
                problems.push(format!(
                    "{} build metadata uses version {}, expected {}",
                    value.build, value.version, first.version
                ));
            }
        }
        validate_changelog(&project.root, &first.version, &mut problems);
    }
    validate_public(project, &mut problems);
    validate_translations(&source_root, &mut problems);
    if release {
        validate_release(project, &mut problems);
    }

    if !problems.is_empty() {
        return Err(Error::validation(problems.join("\n")));
    }
    let result = metadata
        .into_iter()
        .next()
        .ok_or_else(|| Error::validation("no mod metadata was found"))?;
    Ok(ValidatedProject {
        project,
        metadata: result,
    })
}

fn validate_changelog(root: &Path, version: &str, problems: &mut Vec<String>) {
    let path = root.join("CHANGELOG.md");
    match fs::read_to_string(&path) {
        Ok(source) => {
            let heading = Regex::new(r"(?m)^## (\d+\.\d+\.\d+)\s*$").expect("valid regex");
            match heading.captures(&source).and_then(|capture| capture.get(1)) {
                Some(found) if found.as_str() == version => {}
                Some(found) => problems.push(format!(
                    "{}: first release is {}, expected {version}",
                    path.display(),
                    found.as_str()
                )),
                None => problems.push(format!(
                    "{}: add `## {version}` as the first release",
                    path.display()
                )),
            }
        }
        Err(error) => problems.push(format!("{}: {error}", path.display())),
    }
}

fn validate_public(project: &Project, problems: &mut Vec<String>) {
    let public = project.root.join(&project.config.paths.public);
    for name in ["description.md", "preview.png", "workshop.txt"] {
        let path = public.join(name);
        if !path.is_file() {
            problems.push(format!("{}: required file is missing", path.display()));
        }
    }
    let preview = public.join("preview.png");
    if let Ok(data) = fs::read(&preview) {
        if data.len() as u64 >= PREVIEW_MAX_BYTES {
            problems.push(format!(
                "{}: preview must be under 1000 KB",
                preview.display()
            ));
        }
        if data.len() < 24 || &data[..8] != b"\x89PNG\r\n\x1a\n" {
            problems.push(format!("{}: preview is not a valid PNG", preview.display()));
        } else {
            let width = u32::from_be_bytes(data[16..20].try_into().expect("four bytes"));
            let height = u32::from_be_bytes(data[20..24].try_into().expect("four bytes"));
            if (width, height) != (256, 256) {
                problems.push(format!(
                    "{}: preview must be 256x256, found {width}x{height}",
                    preview.display()
                ));
            }
        }
    }
}

fn validate_translations(source_root: &Path, problems: &mut Vec<String>) {
    for entry in WalkDir::new(source_root)
        .into_iter()
        .filter_map(|entry| entry.ok())
    {
        let path = entry.path();
        if path
            .extension()
            .is_some_and(|extension| extension == "json")
            && path
                .components()
                .any(|part| part.as_os_str() == "Translate")
        {
            match fs::read_to_string(path)
                .ok()
                .and_then(|source| serde_json::from_str::<serde_json::Value>(&source).ok())
            {
                Some(serde_json::Value::Object(_)) => {}
                _ => problems.push(format!(
                    "{}: translation must be a valid JSON object",
                    path.display()
                )),
            }
        }
    }
}

fn validate_release(project: &Project, problems: &mut Vec<String>) {
    for included in &project.config.release.include {
        let path = project.root.join(included);
        if !path.is_file() {
            problems.push(format!("{}: release file is missing", path.display()));
        }
    }
    if let Some(minify) = &project.config.release.minify
        && !command_exists(&minify.command)
    {
        problems.push(format!(
            "release minifier `{}` was not found",
            minify.command
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::environment::{command_exists, home_directory};
    use crate::test_runner;
    use std::path::PathBuf;
    use tempfile::tempdir;

    fn valid_project(root: &Path) -> Project {
        fs::create_dir_all(root.join("src/42")).unwrap();
        fs::create_dir_all(root.join("public")).unwrap();
        fs::write(
            root.join("src/42/mod.info"),
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
        let mut png = Vec::from(b"\x89PNG\r\n\x1a\n".as_slice());
        png.extend_from_slice(&[0; 8]);
        png.extend_from_slice(&256_u32.to_be_bytes());
        png.extend_from_slice(&256_u32.to_be_bytes());
        fs::write(root.join("public/preview.png"), png).unwrap();
        Project {
            root: root.to_path_buf(),
            config: Config::default(),
        }
    }

    #[test]
    fn reads_mod_metadata() {
        let temporary = tempdir().unwrap();
        let path = temporary.path().join("mod.info");
        fs::write(&path, "name=Example\nid=Example\nmodversion=1.2.3\n").unwrap();
        let metadata = read_metadata(&path, "42").unwrap();
        assert_eq!(metadata.id, "Example");
        assert_eq!(metadata.version, "1.2.3");
    }

    #[test]
    fn rejects_non_semantic_versions() {
        let temporary = tempdir().unwrap();
        let path = temporary.path().join("mod.info");
        fs::write(&path, "name=Example\nid=Example\nmodversion=1.2\n").unwrap();
        assert!(read_metadata(&path, "42").is_err());
    }

    #[test]
    fn rejects_missing_and_invalid_metadata_fields() {
        let temporary = tempdir().unwrap();
        let path = temporary.path().join("mod.info");
        assert!(read_metadata(&path, "42").is_err());
        fs::write(&path, "name=Example\nid=bad-id\nmodversion=1.0.0\n").unwrap();
        assert!(read_metadata(&path, "42").is_err());
        fs::write(&path, "name=Example\nid=Example\n").unwrap();
        assert!(read_metadata(&path, "42").is_err());
    }

    #[test]
    fn accumulates_cross_build_validation_problems() {
        let temporary = tempdir().unwrap();
        let mut value = valid_project(temporary.path());
        fs::create_dir_all(temporary.path().join("src/41")).unwrap();
        fs::write(
            temporary.path().join("src/41/mod.info"),
            "name=Other\nid=Other\nmodversion=2.0.0\n",
        )
        .unwrap();
        value.config.project.builds.push("41".to_owned());
        let error = check(&value, false).unwrap_err().to_string();
        assert!(error.contains("uses ID Other"));
        assert!(error.contains("uses version 2.0.0"));

        value.config.project.builds.clear();
        assert!(check(&value, false).is_err());
    }

    #[test]
    fn validates_changelog_failure_modes() {
        let temporary = tempdir().unwrap();
        let mut problems = Vec::new();
        validate_changelog(temporary.path(), "1.0.0", &mut problems);
        assert_eq!(problems.len(), 1);
        fs::write(temporary.path().join("CHANGELOG.md"), "no releases").unwrap();
        validate_changelog(temporary.path(), "1.0.0", &mut problems);
        assert!(
            problems
                .iter()
                .any(|problem| problem.contains("add `## 1.0.0`"))
        );
        fs::write(temporary.path().join("CHANGELOG.md"), "## 0.9.0\n").unwrap();
        validate_changelog(temporary.path(), "1.0.0", &mut problems);
        assert!(
            problems
                .iter()
                .any(|problem| problem.contains("expected 1.0.0"))
        );
    }

    #[test]
    fn validates_preview_and_release_failure_modes() {
        let temporary = tempdir().unwrap();
        let mut value = valid_project(temporary.path());
        let preview = temporary.path().join("public/preview.png");
        let mut png = fs::read(&preview).unwrap();
        png[16..20].copy_from_slice(&128_u32.to_be_bytes());
        fs::write(&preview, png).unwrap();
        let mut problems = Vec::new();
        validate_public(&value, &mut problems);
        assert!(problems.iter().any(|problem| problem.contains("128x256")));

        fs::write(&preview, vec![0; PREVIEW_MAX_BYTES as usize]).unwrap();
        validate_public(&value, &mut problems);
        assert!(
            problems
                .iter()
                .any(|problem| problem.contains("under 1000 KB"))
        );

        value.config.release.include.push(PathBuf::from("MISSING"));
        value.config.release.minify = Some(crate::config::MinifyConfig {
            command: "knoxmancer-command-that-does-not-exist".to_owned(),
            args: Vec::new(),
        });
        validate_release(&value, &mut problems);
        assert!(
            problems
                .iter()
                .any(|problem| problem.contains("release file is missing"))
        );
        assert!(
            problems
                .iter()
                .any(|problem| problem.contains("was not found"))
        );
    }

    #[test]
    fn validates_test_configuration_and_project_files() {
        let temporary = tempdir().unwrap();
        let mut value = valid_project(temporary.path());
        let validated = check(&value, false).unwrap();
        assert!(test_runner::run(&validated).is_err());
        value.config.test.command = vec!["knoxmancer-command-that-does-not-exist".to_owned()];
        let validated = check(&value, false).unwrap();
        assert!(test_runner::run(&validated).is_err());

        fs::remove_file(temporary.path().join("public/description.md")).unwrap();
        assert!(check(&value, false).is_err());
        assert!(home_directory().is_some());
        assert!(!command_exists("knoxmancer-command-that-does-not-exist"));
    }
}

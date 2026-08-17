use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use regex::Regex;
use serde::Serialize;
use walkdir::WalkDir;

use crate::cli::{DoctorArgs, TestArgs};
use crate::config::Project;
use crate::error::{Error, Result};
use crate::output::Reporter;

const PREVIEW_MAX_BYTES: u64 = 1_000_000;

#[derive(Debug, Clone, Serialize)]
pub struct ModMetadata {
    pub name: String,
    pub id: String,
    pub version: String,
    pub build: String,
}

pub fn doctor(_: &DoctorArgs, reporter: &Reporter) -> Result<()> {
    reporter.status("Knoxmancer environment");
    report_command(reporter, "git", &["--version"]);
    report_command(reporter, "lua5.1", &["-v"]);
    report_command(reporter, "prometheus-lua", &["--version"]);

    let home = home_directory();
    if let Some(home) = home {
        let mods = home.join("Zomboid/mods");
        reporter.status(&format!(
            "Local mods: {} ({})",
            mods.display(),
            if mods.is_dir() { "found" } else { "not found" }
        ));
    } else {
        reporter.status("Local mods: home directory unavailable");
    }
    Ok(())
}

pub fn check(project: &Project, release: bool, reporter: &Reporter) -> Result<ModMetadata> {
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
    reporter.status(&format!(
        "Checked {} {} ({})",
        result.name,
        result.version,
        project
            .config
            .project
            .builds
            .iter()
            .map(|build| format!("Build {build}"))
            .collect::<Vec<_>>()
            .join(", ")
    ));
    Ok(result)
}

pub fn test(project: &Project, _: &TestArgs, reporter: &Reporter) -> Result<()> {
    check(project, false, reporter)?;
    let (program, arguments) = project
        .config
        .test
        .command
        .split_first()
        .ok_or_else(|| Error::project("no test.command is configured in knoxmancer.toml"))?;
    reporter.status(&format!(
        "Running {}",
        project.config.test.command.join(" ")
    ));
    let status = Command::new(program)
        .args(arguments)
        .current_dir(&project.root)
        .status()
        .map_err(|error| Error::tool(format!("could not run {program}: {error}")))?;
    if !status.success() {
        return Err(Error::tool(format!(
            "test command exited with {}",
            status
                .code()
                .map_or_else(|| "a signal".to_owned(), |code| code.to_string())
        )));
    }
    reporter.status("Tests passed");
    Ok(())
}

pub fn read_metadata(path: &Path, build: &str) -> Result<ModMetadata> {
    let source = fs::read_to_string(path)
        .map_err(|error| Error::validation(format!("{}: {error}", path.display())))?;
    let fields = parse_fields(&source);
    let required = |key: &str| {
        fields
            .get(key)
            .cloned()
            .ok_or_else(|| Error::validation(format!("{}: missing `{key}`", path.display())))
    };
    let version = required("modversion")?;
    let semantic_version = Regex::new(r"^\d+\.\d+\.\d+$").expect("valid regex");
    if !semantic_version.is_match(&version) {
        return Err(Error::validation(format!(
            "{}: modversion `{version}` must use MAJOR.MINOR.PATCH",
            path.display()
        )));
    }
    let id = required("id")?;
    if !id
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return Err(Error::validation(format!(
            "{}: id `{id}` contains unsupported characters",
            path.display()
        )));
    }
    Ok(ModMetadata {
        name: required("name")?,
        id,
        version,
        build: build.to_owned(),
    })
}

fn parse_fields(source: &str) -> BTreeMap<String, String> {
    source
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.trim().to_owned(), value.trim().to_owned()))
        .collect()
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

fn report_command(reporter: &Reporter, name: &str, arguments: &[&str]) {
    match Command::new(name).args(arguments).output() {
        Ok(output) if output.status.success() => {
            let text = if output.stdout.is_empty() {
                &output.stderr
            } else {
                &output.stdout
            };
            reporter.status(&format!("{name}: {}", String::from_utf8_lossy(text).trim()));
        }
        _ => reporter.status(&format!("{name}: not found")),
    }
}

fn command_exists(command: &str) -> bool {
    Command::new(command).arg("--version").output().is_ok()
}

pub fn home_directory() -> Option<PathBuf> {
    env::var_os("USERPROFILE")
        .or_else(|| env::var_os("HOME"))
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

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
}

use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use regex::Regex;
use walkdir::WalkDir;

use crate::cli::{CleanArgs, InstallArgs, PackageArgs};
use crate::config::{MinifyConfig, Project};
use crate::error::{Error, Result};
use crate::output::Reporter;
use crate::validation::{self, ModMetadata};

const DESCRIPTION_MARKER: &str = "{{DESCRIPTION}}";
const CHANGELOG_MARKER: &str = "{{CHANGELOG}}";
const WORKSHOP_DESCRIPTION_MAX_BYTES: usize = 8_000;

pub fn build(project: &Project, release: bool, reporter: &Reporter) -> Result<PathBuf> {
    let metadata = validation::check(project, release, reporter)?;
    let output = output_root(project)?;
    let profile = if release { "release" } else { "dev" };
    let destination = output.join(profile).join(&metadata.id);
    let staging = staging_path(
        destination.parent().expect("profile directory"),
        &metadata.id,
    );
    reporter.verbose(&format!("Staging artifact in {}", staging.display()));

    remove_tree_if_exists(&staging)?;
    fs::create_dir_all(&staging).map_err(Error::io)?;
    let result = (|| {
        copy_mod_source(project, &staging)?;
        copy_public_assets(project, &staging)?;
        for included in &project.config.release.include {
            let source = project.root.join(included);
            if source.is_file() {
                copy_file(&source, &staging.join(included))?;
            }
        }
        if release && let Some(minifier) = &project.config.release.minify {
            let count = minify_lua(&staging, minifier)?;
            reporter.status(&format!("Minified {count} Lua files"));
        }
        atomic_replace(&staging, &destination)
    })();
    if result.is_err() {
        let _ = remove_tree_if_exists(&staging);
    }
    result?;
    reporter.status(&format!(
        "Built {} artifact: {}",
        profile,
        destination.display()
    ));
    Ok(destination)
}

pub fn install(project: &Project, args: &InstallArgs, reporter: &Reporter) -> Result<PathBuf> {
    let built = build(project, args.release, reporter)?;
    let metadata = validation::check(project, args.release, reporter)?;
    let root = match &args.root {
        Some(path) if path.is_absolute() => path.clone(),
        Some(path) => std::env::current_dir().map_err(Error::io)?.join(path),
        None => validation::home_directory()
            .ok_or_else(|| Error::project("home directory is unavailable; pass --root"))?
            .join("Zomboid/mods"),
    };
    let destination = root.join(&metadata.id);
    if destination.parent() != Some(root.as_path()) {
        return Err(Error::project(format!(
            "unsafe install destination: {}",
            destination.display()
        )));
    }
    fs::create_dir_all(&root).map_err(Error::io)?;
    let staging = staging_path(&root, &metadata.id);
    remove_tree_if_exists(&staging)?;
    copy_tree(&built, &staging)?;
    atomic_replace(&staging, &destination)?;
    reporter.status(&format!("Installed {}", destination.display()));
    Ok(destination)
}

pub fn package(project: &Project, _: &PackageArgs, reporter: &Reporter) -> Result<PathBuf> {
    let release = build(project, true, reporter)?;
    let metadata = validation::check(project, true, reporter)?;
    let output = output_root(project)?;
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
        for directory in project
            .config
            .project
            .builds
            .iter()
            .map(String::as_str)
            .chain(std::iter::once("common"))
        {
            let source = release.join(directory);
            if source.is_dir() {
                copy_tree(&source, &mod_root.join(directory))?;
            }
        }
        for included in &project.config.release.include {
            let source = release.join(included);
            if source.is_file() {
                let file_name = included.file_name().ok_or_else(|| {
                    Error::project(format!("invalid included path: {}", included.display()))
                })?;
                if file_name == "LICENSE" {
                    copy_file(&source, &mod_root.join(file_name))?;
                } else {
                    copy_file(&source, &staging.join(file_name))?;
                }
            }
        }
        let public = project.root.join(&project.config.paths.public);
        copy_file(&public.join("preview.png"), &staging.join("preview.png"))?;
        fs::write(
            staging.join("workshop.txt"),
            render_workshop(project, &metadata)?,
        )
        .map_err(Error::io)?;
        atomic_replace(&staging, &destination)
    })();
    if result.is_err() {
        let _ = remove_tree_if_exists(&staging);
    }
    result?;
    reporter.status(&format!(
        "Packaged Workshop artifact: {}",
        destination.display()
    ));
    Ok(destination)
}

pub fn clean(project: &Project, _: &CleanArgs, reporter: &Reporter) -> Result<()> {
    let output = output_root(project)?;
    if output.exists() {
        remove_tree_if_exists(&output)?;
        reporter.status(&format!("Removed {}", output.display()));
    } else {
        reporter.status(&format!("Nothing to clean: {}", output.display()));
    }
    Ok(())
}

fn output_root(project: &Project) -> Result<PathBuf> {
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

fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    if !source.is_dir() {
        return Err(Error::project(format!(
            "source directory is missing: {}",
            source.display()
        )));
    }
    fs::create_dir_all(destination).map_err(Error::io)?;
    for entry in WalkDir::new(source).follow_links(false) {
        let entry = entry.map_err(|error| Error::io(std::io::Error::other(error)))?;
        let relative = entry
            .path()
            .strip_prefix(source)
            .expect("walked entry is below source");
        let target = destination.join(relative);
        if entry.file_type().is_symlink() {
            return Err(Error::project(format!(
                "symbolic links are not supported in artifacts: {}",
                entry.path().display()
            )));
        }
        if entry.file_type().is_dir() {
            fs::create_dir_all(&target).map_err(Error::io)?;
        } else if entry.file_type().is_file() {
            copy_file(entry.path(), &target)?;
        }
    }
    Ok(())
}

fn copy_file(source: &Path, destination: &Path) -> Result<()> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(Error::io)?;
    }
    fs::copy(source, destination).map(|_| ()).map_err(|error| {
        Error::io(std::io::Error::new(
            error.kind(),
            format!("{}: {error}", source.display()),
        ))
    })
}

fn atomic_replace(staging: &Path, destination: &Path) -> Result<()> {
    let parent = destination
        .parent()
        .ok_or_else(|| Error::project("artifact destination has no parent"))?;
    fs::create_dir_all(parent).map_err(Error::io)?;
    let backup = parent.join(format!(
        ".{}-backup-{}",
        destination
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("artifact"),
        unique_token()
    ));
    if destination.exists() {
        fs::rename(destination, &backup).map_err(Error::io)?;
    }
    if let Err(error) = fs::rename(staging, destination) {
        if backup.exists() && !destination.exists() {
            let _ = fs::rename(&backup, destination);
        }
        return Err(Error::io(error));
    }
    remove_tree_if_exists(&backup)
}

fn remove_tree_if_exists(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    for entry in WalkDir::new(path)
        .contents_first(true)
        .into_iter()
        .flatten()
    {
        if let Ok(metadata) = entry.path().metadata() {
            let mut permissions = metadata.permissions();
            if permissions.readonly() {
                make_writable(&mut permissions);
                let _ = fs::set_permissions(entry.path(), permissions);
            }
        }
    }
    fs::remove_dir_all(path).map_err(Error::io)
}

#[cfg(windows)]
fn make_writable(permissions: &mut fs::Permissions) {
    permissions.set_readonly(false);
}

#[cfg(unix)]
fn make_writable(permissions: &mut fs::Permissions) {
    use std::os::unix::fs::PermissionsExt;
    permissions.set_mode(permissions.mode() | 0o200);
}

fn staging_path(parent: &Path, id: &str) -> PathBuf {
    parent.join(format!(".{id}-staging-{}", unique_token()))
}

fn unique_token() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
        ^ u128::from(std::process::id())
}

fn minify_lua(root: &Path, config: &MinifyConfig) -> Result<usize> {
    let files: Vec<_> = WalkDir::new(root)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry.file_type().is_file()
                && entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "lua")
        })
        .map(|entry| entry.into_path())
        .collect();
    for source in &files {
        let generated = source.with_extension("lua.knoxmancer");
        let arguments: Vec<_> = config
            .args
            .iter()
            .map(|argument| {
                argument
                    .replace("{input}", &source.to_string_lossy())
                    .replace("{output}", &generated.to_string_lossy())
            })
            .collect();
        let status = Command::new(&config.command)
            .args(&arguments)
            .status()
            .map_err(|error| Error::tool(format!("could not run {}: {error}", config.command)))?;
        if !status.success() {
            let _ = fs::remove_file(&generated);
            return Err(Error::tool(format!(
                "minifier failed for {}",
                source.display()
            )));
        }
        if config
            .args
            .iter()
            .any(|argument| argument.contains("{output}"))
        {
            if !generated.is_file() || generated.metadata().map_err(Error::io)?.len() == 0 {
                return Err(Error::tool(format!(
                    "minifier produced no output for {}",
                    source.display()
                )));
            }
            fs::rename(&generated, source).map_err(Error::io)?;
        }
    }
    Ok(files.len())
}

fn render_workshop(project: &Project, metadata: &ModMetadata) -> Result<String> {
    let public = project.root.join(&project.config.paths.public);
    let description_path = public.join("description.md");
    let description_template = fs::read_to_string(&description_path).map_err(Error::io)?;
    let marker_count = description_template.matches(CHANGELOG_MARKER).count();
    let rendered_markdown = if marker_count == 0 {
        description_template
    } else if marker_count == 1 && description_template.trim_end().ends_with(CHANGELOG_MARKER) {
        let changelog = fs::read_to_string(project.root.join("CHANGELOG.md")).map_err(Error::io)?;
        let releases = release_history(&changelog, &metadata.version)?;
        description_template.replace(CHANGELOG_MARKER, &releases)
    } else {
        return Err(Error::validation(format!(
            "{}: {CHANGELOG_MARKER} must appear at most once and as the final content",
            description_path.display()
        )));
    };
    let description = markdown_to_bbcode(&rendered_markdown);
    if description.len() >= WORKSHOP_DESCRIPTION_MAX_BYTES {
        return Err(Error::validation(format!(
            "Workshop description must be under {WORKSHOP_DESCRIPTION_MAX_BYTES} bytes; found {}",
            description.len()
        )));
    }
    let description_lines = description
        .lines()
        .map(|line| format!("description={line}"))
        .collect::<Vec<_>>()
        .join("\n");
    let workshop_path = public.join("workshop.txt");
    let workshop = fs::read_to_string(&workshop_path).map_err(Error::io)?;
    if workshop.matches(DESCRIPTION_MARKER).count() != 1 {
        return Err(Error::validation(format!(
            "{}: {DESCRIPTION_MARKER} must appear exactly once",
            workshop_path.display()
        )));
    }
    Ok(workshop.replace(DESCRIPTION_MARKER, &description_lines))
}

fn release_history(changelog: &str, current_version: &str) -> Result<String> {
    let heading = Regex::new(r"(?m)^## (\d+\.\d+\.\d+)\s*$").expect("valid regex");
    let headings: Vec<_> = heading.captures_iter(changelog).collect();
    if headings
        .first()
        .and_then(|capture| capture.get(1))
        .map(|value| value.as_str())
        != Some(current_version)
    {
        return Err(Error::validation(format!(
            "CHANGELOG.md must begin with version {current_version}"
        )));
    }
    let mut output = Vec::new();
    for (index, capture) in headings.iter().enumerate() {
        let complete = capture.get(0).expect("complete match");
        let end = headings
            .get(index + 1)
            .and_then(|next| next.get(0))
            .map_or(changelog.len(), |next| next.start());
        let notes = changelog[complete.end()..end]
            .lines()
            .filter(|line| line.starts_with("- "))
            .collect::<Vec<_>>();
        if notes.is_empty() {
            return Err(Error::validation(format!(
                "CHANGELOG.md release {} has no notes",
                capture.get(1).expect("version").as_str()
            )));
        }
        output.push(format!(
            "### {}\n{}",
            capture.get(1).expect("version").as_str(),
            notes.join("\n")
        ));
    }
    Ok(output.join("\n\n"))
}

fn markdown_to_bbcode(markdown: &str) -> String {
    let link = Regex::new(r"\[([^]]+)]\((https?://[^)]+)\)").expect("valid regex");
    let bold = Regex::new(r"\*\*(.+?)\*\*").expect("valid regex");
    let italic = Regex::new(r"\*([^*]+)\*").expect("valid regex");
    let code = Regex::new(r"`([^`]+)`").expect("valid regex");
    let mut output = Vec::new();
    let mut in_list = false;
    for line in markdown.lines() {
        if let Some(item) = line.strip_prefix("- ") {
            if !in_list {
                output.push("[list]".to_owned());
                in_list = true;
            }
            output.push(format!("[*]{}", inline(item, &link, &bold, &italic, &code)));
            continue;
        }
        if in_list {
            output.push("[/list]".to_owned());
            in_list = false;
        }
        let rendered = if let Some(text) = line.strip_prefix("### ") {
            format!("[h3]{}[/h3]", inline(text, &link, &bold, &italic, &code))
        } else if let Some(text) = line.strip_prefix("## ") {
            format!("[h2]{}[/h2]", inline(text, &link, &bold, &italic, &code))
        } else if let Some(text) = line.strip_prefix("# ") {
            format!("[h1]{}[/h1]", inline(text, &link, &bold, &italic, &code))
        } else {
            inline(line, &link, &bold, &italic, &code)
        };
        output.push(rendered);
    }
    if in_list {
        output.push("[/list]".to_owned());
    }
    output.join("\n")
}

fn inline(text: &str, link: &Regex, bold: &Regex, italic: &Regex, code: &Regex) -> String {
    let text = link.replace_all(text, "[url=$2]$1[/url]");
    let text = bold.replace_all(&text, "[b]$1[/b]");
    let text = italic.replace_all(&text, "[i]$1[/i]");
    code.replace_all(&text, "[code]$1[/code]").into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

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
}

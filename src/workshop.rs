//! Steam Workshop description rendering.

use std::fs;

use regex::Regex;

use crate::artifact::ReleaseArtifact;
use crate::config::Project;
use crate::error::{Error, Result};
use crate::filesystem::{
    atomic_replace, copy_file, copy_tree, remove_tree_if_exists, staging_path,
};
use crate::layout::ProjectLayout;
use crate::metadata::ModMetadata;
use crate::validation::ValidatedProject;

const DESCRIPTION_MARKER: &str = "{{DESCRIPTION}}";
const CHANGELOG_MARKER: &str = "{{CHANGELOG}}";
pub(crate) const DESCRIPTION_MAX_BYTES: usize = 8_000;

pub(crate) fn package(
    validated: &ValidatedProject<'_>,
    release: &ReleaseArtifact,
) -> Result<PackageResult> {
    let project = validated.project;
    let metadata = &validated.metadata;
    let release = release.artifact();
    if release.mod_id != metadata.id {
        return Err(Error::project(format!(
            "release artifact ID {} does not match validated mod ID {}",
            release.mod_id, metadata.id
        )));
    }
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
        for directory in project
            .config
            .project
            .builds
            .iter()
            .map(String::as_str)
            .chain(std::iter::once("common"))
        {
            let source = release.path.join(directory);
            if source.is_dir() {
                copy_tree(&source, &mod_root.join(directory))?;
            }
        }
        for included in &project.config.release.include {
            let (relative, _) = validated.layout.included(included)?;
            let source = release.path.join(&relative);
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
        fs::write(staging.join("workshop.txt"), render(project, metadata)?).map_err(Error::io)?;
        atomic_replace(&staging, &destination)
    })();
    if result.is_err() {
        let _ = remove_tree_if_exists(&staging);
    }
    let replacement = result?;
    Ok(PackageResult {
        path: destination,
        warnings: replacement.cleanup_warning.into_iter().collect(),
    })
}

/// Result of assembling a Steam Workshop upload tree.
#[derive(Debug)]
pub(crate) struct PackageResult {
    pub path: std::path::PathBuf,
    pub warnings: Vec<String>,
}

pub(crate) fn render(project: &Project, metadata: &ModMetadata) -> Result<String> {
    let public = ProjectLayout::new(project)?.public_root()?;
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
    if description.len() >= DESCRIPTION_MAX_BYTES {
        return Err(Error::validation(format!(
            "Workshop description must be under {DESCRIPTION_MAX_BYTES} bytes; found {}",
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

pub(crate) fn release_history(changelog: &str, current_version: &str) -> Result<String> {
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

pub(crate) fn markdown_to_bbcode(markdown: &str) -> String {
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

//! Workshop description and changelog rendering.

use std::fs;

use regex::Regex;

use crate::error::{Error, Result};
use crate::project::{ModMetadata, Project, ProjectLayout};

const DESCRIPTION_MARKER: &str = "{{DESCRIPTION}}";
const CHANGELOG_MARKER: &str = "{{CHANGELOG}}";
pub(crate) const DESCRIPTION_MAX_BYTES: usize = 8_000;

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
    if description.trim().is_empty() {
        return Err(Error::validation(format!(
            "{}: Workshop description must not be empty",
            description_path.display()
        )));
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::config::Config;
    use tempfile::tempdir;

    fn project(root: &std::path::Path) -> Project {
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
    fn validates_templates_changelog_and_limits() {
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
            render(&value, &metadata)
                .unwrap()
                .contains("description=plain")
        );
        fs::write(public.join("description.md"), "\n").unwrap();
        assert!(render(&value, &metadata).is_err());
        fs::write(public.join("description.md"), "{{CHANGELOG}}\ntrailing").unwrap();
        assert!(render(&value, &metadata).is_err());
        fs::write(
            public.join("description.md"),
            "x".repeat(DESCRIPTION_MAX_BYTES),
        )
        .unwrap();
        assert!(render(&value, &metadata).is_err());
        fs::write(public.join("description.md"), "plain").unwrap();
        fs::write(public.join("workshop.txt"), "missing marker").unwrap();
        assert!(render(&value, &metadata).is_err());

        assert!(release_history("## 0.9.0\n\n- Old", "1.0.0").is_err());
        assert!(release_history("## 1.0.0\n\nNo note", "1.0.0").is_err());
        assert_eq!(
            release_history("## 1.0.0\n\n- New\n\n## 0.9.0\n\n- Old", "1.0.0").unwrap(),
            "### 1.0.0\n- New\n\n### 0.9.0\n- Old"
        );
    }
}

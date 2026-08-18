//! Workshop description rendering.

use std::fs;

use regex::Regex;

use crate::error::{Error, Result};
use crate::project::{Project, ProjectLayout, WorkshopMetadata};
/// Maximum UTF-8 byte length accepted by Steam Workshop.
pub(crate) const DESCRIPTION_MAX_BYTES: usize = 8_000;

/// Renders the public Markdown description into `workshop.txt`.
pub(crate) fn render(project: &Project, metadata: &WorkshopMetadata) -> Result<String> {
    let public = ProjectLayout::new(project)?.public_root()?;
    let description_path = public.join("description.md");
    let markdown = fs::read_to_string(&description_path).map_err(Error::io)?;
    if markdown.contains("{{") || markdown.contains("}}") {
        return Err(Error::validation(format!(
            "{}: description contains an unsupported template marker",
            description_path.display()
        )));
    }
    let description = markdown_to_bbcode(&markdown);
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
    Ok(metadata.render(&description_lines))
}

/// Converts the supported Markdown subset into Steam BBCode.
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

/// Converts supported inline Markdown constructs into Steam BBCode.
fn inline(text: &str, link: &Regex, bold: &Regex, italic: &Regex, code: &Regex) -> String {
    let text = link.replace_all(text, "[url=$2]$1[/url]");
    let text = bold.replace_all(&text, "[b]$1[/b]");
    let text = italic.replace_all(&text, "[i]$1[/i]");
    code.replace_all(&text, "[code]$1[/code]").into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::WorkshopVisibility;
    use crate::project::config::Config;
    use tempfile::tempdir;

    fn project(root: &std::path::Path) -> Project {
        Project {
            root: root.to_path_buf(),
            config: Config::default(),
        }
    }

    fn metadata() -> WorkshopMetadata {
        WorkshopMetadata {
            id: 0,
            title: "Example".to_owned(),
            tags: vec!["Build 42".to_owned()],
            visibility: WorkshopVisibility::Unlisted,
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
    fn validates_templates_and_limits() {
        let temporary = tempdir().unwrap();
        let value = project(temporary.path());
        let public = temporary.path().join("public");
        fs::create_dir(&public).unwrap();
        fs::write(public.join("description.md"), "plain").unwrap();
        assert!(
            render(&value, &metadata())
                .unwrap()
                .contains("description=plain")
        );
        fs::write(public.join("description.md"), "\n").unwrap();
        assert!(render(&value, &metadata()).is_err());
        fs::write(public.join("description.md"), "{{REMOVED}}").unwrap();
        assert!(render(&value, &metadata()).is_err());
        fs::write(
            public.join("description.md"),
            "x".repeat(DESCRIPTION_MAX_BYTES),
        )
        .unwrap();
        assert!(render(&value, &metadata()).is_err());
    }
}

//! Workshop description rendering.

use std::fs;

use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd};

use crate::error::{Error, Result};
use crate::project::{Project, ProjectLayout, WorkshopMetadata};

/// Maximum UTF-8 byte length accepted by Steam Workshop.
pub(crate) const DESCRIPTION_MAX_BYTES: usize = 8_000;

/// Renders the public Markdown description into `workshop.txt`.
pub(crate) fn render(project: &Project, metadata: &WorkshopMetadata) -> Result<String> {
    let public = ProjectLayout::new(project)?.public_root()?;
    let description_path = public.join("description.md");
    let markdown = fs::read_to_string(&description_path).map_err(Error::io)?;
    let description = markdown_to_bbcode(&markdown).map_err(|message| {
        Error::validation(format!("{}: {message}", description_path.display()))
    })?;
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

/// Converts the supported CommonMark subset into Steam BBCode.
pub(crate) fn markdown_to_bbcode(markdown: &str) -> std::result::Result<String, String> {
    let mut renderer = Renderer::default();
    for event in Parser::new(markdown) {
        renderer.event(event)?;
    }
    Ok(renderer.output.trim_end().to_owned())
}

/// Stateful CommonMark-event to BBCode renderer.
#[derive(Default)]
struct Renderer {
    /// Accumulated BBCode.
    output: String,
    /// Number of currently open unordered lists.
    list_depth: usize,
    /// Whether content is currently inside a list item.
    in_item: bool,
}

impl Renderer {
    /// Renders one parser event or rejects a construct outside the supported subset.
    fn event(&mut self, event: Event<'_>) -> std::result::Result<(), String> {
        match event {
            Event::Start(tag) => self.start(tag)?,
            Event::End(tag) => self.end(tag)?,
            Event::Text(text) => self.text(&text),
            Event::Code(code) => {
                self.output.push_str("[code]");
                self.text(&code);
                self.output.push_str("[/code]");
            }
            Event::SoftBreak | Event::HardBreak => self.output.push('\n'),
            Event::Rule => {
                self.block_separator();
                self.output.push_str("[hr][/hr]\n\n");
            }
            Event::Html(_) | Event::InlineHtml(_) => {
                return Err("raw HTML is not supported in Workshop descriptions".to_owned());
            }
            Event::InlineMath(_)
            | Event::DisplayMath(_)
            | Event::FootnoteReference(_)
            | Event::TaskListMarker(_) => {
                return Err("unsupported Markdown construct in Workshop description".to_owned());
            }
        }
        Ok(())
    }

    /// Renders the opening of a supported container.
    fn start(&mut self, tag: Tag<'_>) -> std::result::Result<(), String> {
        match tag {
            Tag::Paragraph => {
                if !self.in_item {
                    self.block_separator();
                }
            }
            Tag::Heading { level, .. } => {
                self.block_separator();
                self.output.push_str(heading_open(level)?);
            }
            Tag::List(None) => {
                if self.list_depth != 0 {
                    return Err("nested lists are not supported".to_owned());
                }
                self.block_separator();
                self.output.push_str("[list]\n");
                self.list_depth += 1;
            }
            Tag::List(Some(_)) => {
                return Err("ordered lists are not supported; use `-` bullets".to_owned());
            }
            Tag::Item => {
                self.output.push_str("[*]");
                self.in_item = true;
            }
            Tag::Emphasis => self.output.push_str("[i]"),
            Tag::Strong => self.output.push_str("[b]"),
            Tag::Link { dest_url, .. } => {
                if !(dest_url.starts_with("https://") || dest_url.starts_with("http://"))
                    || dest_url
                        .chars()
                        .any(|character| character.is_control() || character == ']')
                {
                    return Err(format!("unsupported or unsafe link URL: {dest_url}"));
                }
                self.output.push_str("[url=");
                self.output.push_str(&dest_url);
                self.output.push(']');
            }
            Tag::CodeBlock(_) => {
                self.block_separator();
                self.output.push_str("[code]\n");
            }
            _ => return Err("unsupported Markdown block in Workshop description".to_owned()),
        }
        Ok(())
    }

    /// Renders the close of a supported container.
    fn end(&mut self, tag: TagEnd) -> std::result::Result<(), String> {
        match tag {
            TagEnd::Paragraph => {
                if !self.in_item {
                    self.output.push_str("\n\n");
                }
            }
            TagEnd::Heading(level) => {
                self.output.push_str(heading_close(level)?);
                self.output.push_str("\n\n");
            }
            TagEnd::List(false) => {
                self.list_depth = self.list_depth.saturating_sub(1);
                self.output.push_str("[/list]\n\n");
            }
            TagEnd::List(true) => {
                return Err("ordered lists are not supported; use `-` bullets".to_owned());
            }
            TagEnd::Item => {
                self.in_item = false;
                self.output.push('\n');
            }
            TagEnd::Emphasis => self.output.push_str("[/i]"),
            TagEnd::Strong => self.output.push_str("[/b]"),
            TagEnd::Link => self.output.push_str("[/url]"),
            TagEnd::CodeBlock => self.output.push_str("\n[/code]\n\n"),
            _ => return Err("unsupported Markdown block in Workshop description".to_owned()),
        }
        Ok(())
    }

    /// Appends text while preventing it from opening or closing raw BBCode tags.
    fn text(&mut self, text: &str) {
        for character in text.chars() {
            match character {
                '[' => self.output.push_str("&#91;"),
                ']' => self.output.push_str("&#93;"),
                _ => self.output.push(character),
            }
        }
    }

    /// Ensures the next block begins after exactly one blank line.
    fn block_separator(&mut self) {
        if self.output.is_empty() || self.output.ends_with("\n\n") {
            return;
        }
        if !self.output.ends_with('\n') {
            self.output.push('\n');
        }
        self.output.push('\n');
    }
}

/// Returns the BBCode opening tag for one supported heading level.
fn heading_open(level: HeadingLevel) -> std::result::Result<&'static str, String> {
    match level {
        HeadingLevel::H1 => Ok("[h1]"),
        HeadingLevel::H2 => Ok("[h2]"),
        HeadingLevel::H3 => Ok("[h3]"),
        _ => Err("only heading levels 1 through 3 are supported".to_owned()),
    }
}

/// Returns the BBCode closing tag for one supported heading level.
fn heading_close(level: HeadingLevel) -> std::result::Result<&'static str, String> {
    match level {
        HeadingLevel::H1 => Ok("[/h1]"),
        HeadingLevel::H2 => Ok("[/h2]"),
        HeadingLevel::H3 => Ok("[/h3]"),
        _ => Err("only heading levels 1 through 3 are supported".to_owned()),
    }
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
    fn converts_supported_commonmark_without_reinterpreting_code() {
        let rendered = markdown_to_bbcode(
            "# Top\n## Heading\n### Subheading\n\n**bold** and *italic* with [link](https://example.com/a_b) and `*literal* [b]`\nsoft\nbreak  \nhard break\n\n---\n\n- one\n- two\n\n```lua\nreturn [b]\n```\n\nafter",
        )
        .unwrap();
        assert!(rendered.contains("[h1]Top[/h1]"));
        assert!(rendered.contains("[h2]Heading[/h2]"));
        assert!(rendered.contains("[h3]Subheading[/h3]"));
        assert!(rendered.contains("[b]bold[/b]"));
        assert!(rendered.contains("[i]italic[/i]"));
        assert!(rendered.contains("[url=https://example.com/a_b]link[/url]"));
        assert!(rendered.contains("[code]*literal* &#91;b&#93;[/code]"));
        assert!(rendered.contains("[hr][/hr]"));
        assert!(rendered.contains("[list]\n[*]one\n[*]two\n[/list]"));
        assert!(rendered.contains("[code]\nreturn &#91;b&#93;\n\n[/code]"));
        assert!(rendered.ends_with("after"));
    }

    #[test]
    fn rejects_unsupported_or_unsafe_markdown() {
        for markdown in [
            "#### unsupported",
            "1. ordered",
            "<b>html</b>",
            "[unsafe](file:///tmp/example)",
            "![image](https://example.com/image.png)",
            "- outer\n  - nested",
        ] {
            assert!(markdown_to_bbcode(markdown).is_err(), "accepted {markdown}");
        }
    }

    #[test]
    fn rejects_parser_events_outside_the_supported_commonmark_subset() {
        let mut renderer = Renderer::default();
        for event in [
            Event::InlineMath("math".into()),
            Event::DisplayMath("math".into()),
            Event::FootnoteReference("note".into()),
            Event::TaskListMarker(true),
        ] {
            assert!(renderer.event(event).is_err());
        }
        assert!(renderer.end(TagEnd::List(true)).is_err());
        assert!(renderer.end(TagEnd::Image).is_err());
        assert!(heading_close(HeadingLevel::H4).is_err());

        renderer.output = "text".to_owned();
        renderer.block_separator();
        assert_eq!(renderer.output, "text\n\n");
        renderer.output = "text\n".to_owned();
        renderer.block_separator();
        assert_eq!(renderer.output, "text\n\n");
    }

    #[test]
    fn validates_description_content_and_limits() {
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
        fs::write(public.join("description.md"), "<b>html</b>").unwrap();
        assert!(render(&value, &metadata()).is_err());
        fs::write(
            public.join("description.md"),
            "x".repeat(DESCRIPTION_MAX_BYTES),
        )
        .unwrap();
        assert!(render(&value, &metadata()).is_err());
    }
}

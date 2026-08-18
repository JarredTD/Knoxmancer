//! Typed Steam Workshop metadata parsing and rendering.

use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::path::Path;

use super::Diagnostic;

/// Placeholder required in source metadata and replaced during packaging.
const DESCRIPTION_MARKER: &str = "{{DESCRIPTION}}";
/// Workshop fields supported by Knoxmancer.
const FIELDS: [&str; 5] = ["version", "id", "title", "tags", "visibility"];
/// Maximum Workshop title length accepted by Steam, excluding its null terminator.
const TITLE_MAX_BYTES: usize = 128;
/// Maximum length accepted by Steam for one Workshop tag.
const TAG_MAX_BYTES: usize = 255;

/// Validated metadata used to render a canonical `workshop.txt`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorkshopMetadata {
    /// Numeric Steam Workshop item identifier, or zero for a new item.
    pub id: u64,
    /// Display title shown by Steam Workshop.
    pub title: String,
    /// Search and compatibility tags assigned to the item.
    pub tags: Vec<String>,
    /// Steam Workshop visibility setting.
    pub visibility: WorkshopVisibility,
}

impl WorkshopMetadata {
    /// Renders canonical Workshop metadata with pre-rendered description lines.
    pub(crate) fn render(&self, description_lines: &str) -> String {
        format!(
            "version=1\nid={}\ntitle={}\n{}\ntags={}\nvisibility={}\n",
            self.id,
            self.title,
            description_lines,
            self.tags.join(";"),
            self.visibility
        )
    }
}

/// Visibility values accepted by Project Zomboid's Workshop uploader.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkshopVisibility {
    /// Visible to everyone.
    Public,
    /// Visible only to Steam friends.
    FriendsOnly,
    /// Visible only to the owner.
    Private,
    /// Accessible by direct link but omitted from discovery.
    Unlisted,
}

impl fmt::Display for WorkshopVisibility {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Public => "public",
            Self::FriendsOnly => "friendsOnly",
            Self::Private => "private",
            Self::Unlisted => "unlisted",
        })
    }
}

/// Parses and validates supported `workshop.txt` fields.
pub(super) fn parse(path: &Path) -> Result<WorkshopMetadata, Vec<Diagnostic>> {
    let source = fs::read_to_string(path).map_err(|error| {
        vec![Diagnostic::at(
            "workshop.unreadable",
            path,
            error.to_string(),
        )]
    })?;
    let mut diagnostics = Vec::new();
    let mut entries = Vec::new();
    for line in source.lines().filter(|line| !line.trim().is_empty()) {
        if line == DESCRIPTION_MARKER {
            continue;
        }
        match line.split_once('=') {
            Some((key, value)) if !key.is_empty() => {
                if !FIELDS.contains(&key) {
                    diagnostics.push(Diagnostic::at(
                        "workshop.field.unknown",
                        path,
                        format!("unsupported metadata field: {key}"),
                    ));
                }
                entries.push((key, value));
            }
            _ => diagnostics.push(Diagnostic::at(
                "workshop.line.invalid",
                path,
                format!("invalid metadata line: {line}"),
            )),
        }
    }

    let version = one(&entries, "version", path, &mut diagnostics);
    if version.is_some_and(|value| value != "1") {
        diagnostics.push(Diagnostic::at(
            "workshop.value.invalid",
            path,
            format!("version must be 1, found {}", version.unwrap()),
        ));
    }
    let id = one(&entries, "id", path, &mut diagnostics).and_then(|value| {
        value.parse::<u64>().map_err(|_| ()).map_or_else(
            |()| {
                diagnostics.push(Diagnostic::at(
                    "workshop.id.invalid",
                    path,
                    format!("id must be an unsigned integer, found {value}"),
                ));
                None
            },
            Some,
        )
    });
    let title = nonempty(&entries, "title", path, &mut diagnostics).and_then(|value| {
        let value = value.trim();
        if value.len() > TITLE_MAX_BYTES {
            diagnostics.push(Diagnostic::at(
                "workshop.title.too_long",
                path,
                format!("title must not exceed {TITLE_MAX_BYTES} UTF-8 bytes"),
            ));
            None
        } else if value.chars().any(char::is_control) {
            diagnostics.push(Diagnostic::at(
                "workshop.title.invalid",
                path,
                "title must not contain control characters",
            ));
            None
        } else {
            Some(value.to_owned())
        }
    });
    let tags = nonempty(&entries, "tags", path, &mut diagnostics)
        .and_then(|value| parse_tags(value, path, &mut diagnostics));
    let visibility =
        one(&entries, "visibility", path, &mut diagnostics).and_then(|value| match value {
            "public" => Some(WorkshopVisibility::Public),
            "friendsOnly" => Some(WorkshopVisibility::FriendsOnly),
            "private" => Some(WorkshopVisibility::Private),
            "unlisted" => Some(WorkshopVisibility::Unlisted),
            _ => {
                diagnostics.push(Diagnostic::at(
                    "workshop.visibility.invalid",
                    path,
                    format!("unsupported visibility: {value}"),
                ));
                None
            }
        });
    if source.matches(DESCRIPTION_MARKER).count() != 1 {
        diagnostics.push(Diagnostic::at(
            "workshop.description_marker.invalid",
            path,
            format!("{DESCRIPTION_MARKER} must appear exactly once"),
        ));
    }

    match (diagnostics.is_empty(), id, title, tags, visibility) {
        (true, Some(id), Some(title), Some(tags), Some(visibility)) => Ok(WorkshopMetadata {
            id,
            title,
            tags,
            visibility,
        }),
        _ => Err(diagnostics),
    }
}

/// Parses, normalizes, and validates the semicolon-delimited Workshop tag list.
fn parse_tags(value: &str, path: &Path, diagnostics: &mut Vec<Diagnostic>) -> Option<Vec<String>> {
    let mut tags = Vec::new();
    let mut seen = BTreeSet::new();
    for candidate in value.split(';') {
        let tag = candidate.trim();
        if tag.is_empty() {
            diagnostics.push(Diagnostic::at(
                "workshop.tag.empty",
                path,
                "tags must not contain empty entries",
            ));
            continue;
        }
        if tag.len() > TAG_MAX_BYTES {
            diagnostics.push(Diagnostic::at(
                "workshop.tag.too_long",
                path,
                format!("tag must not exceed {TAG_MAX_BYTES} UTF-8 bytes: {tag}"),
            ));
            continue;
        }
        if tag
            .chars()
            .any(|character| character.is_control() || character == ',')
        {
            diagnostics.push(Diagnostic::at(
                "workshop.tag.invalid",
                path,
                format!("tag contains unsupported characters: {tag}"),
            ));
            continue;
        }
        if !seen.insert(tag.to_ascii_lowercase()) {
            diagnostics.push(Diagnostic::at(
                "workshop.tag.duplicate",
                path,
                format!("duplicate tag: {tag}"),
            ));
            continue;
        }
        tags.push(tag.to_owned());
    }
    (!tags.is_empty()).then_some(tags)
}

/// Returns one value for a required field and diagnoses missing or duplicate entries.
fn one<'a>(
    entries: &'a [(&str, &str)],
    key: &str,
    path: &Path,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<&'a str> {
    let values = entries
        .iter()
        .filter_map(|(candidate, value)| (*candidate == key).then_some(*value))
        .collect::<Vec<_>>();
    match values.as_slice() {
        [value] => Some(*value),
        [] => {
            diagnostics.push(Diagnostic::at(
                "workshop.field.missing",
                path,
                format!("{key} is required"),
            ));
            None
        }
        _ => {
            diagnostics.push(Diagnostic::at(
                "workshop.field.duplicate",
                path,
                format!("{key} must appear exactly once"),
            ));
            None
        }
    }
}

/// Returns one required non-empty field value.
fn nonempty<'a>(
    entries: &'a [(&str, &str)],
    key: &str,
    path: &Path,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<&'a str> {
    one(entries, key, path, diagnostics).and_then(|value| {
        if value.trim().is_empty() {
            diagnostics.push(Diagnostic::at(
                "workshop.value.empty",
                path,
                format!("{key} must not be empty"),
            ));
            None
        } else {
            Some(value)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn parses_and_renders_canonical_metadata() {
        let temporary = tempdir().unwrap();
        let path = temporary.path().join("workshop.txt");
        fs::write(
            &path,
            "version=1\nid=0\ntitle=Example\n{{DESCRIPTION}}\ntags=Build 42;Multiplayer\nvisibility=unlisted\n",
        )
        .unwrap();
        let metadata = parse(&path).unwrap();
        assert_eq!(metadata.tags, ["Build 42", "Multiplayer"]);
        assert_eq!(
            metadata.render("description=Example"),
            "version=1\nid=0\ntitle=Example\ndescription=Example\ntags=Build 42;Multiplayer\nvisibility=unlisted\n"
        );
    }

    #[test]
    fn reports_structural_and_field_problems() {
        let temporary = tempdir().unwrap();
        let path = temporary.path().join("workshop.txt");
        fs::write(
            &path,
            "broken\nversion=2\nid=nope\ntitle=\ntitle=again\ntags=\nvisibility=hidden\nextra=value\n",
        )
        .unwrap();
        let diagnostics = parse(&path).unwrap_err();
        for code in [
            "workshop.line.invalid",
            "workshop.field.unknown",
            "workshop.value.invalid",
            "workshop.id.invalid",
            "workshop.field.duplicate",
            "workshop.value.empty",
            "workshop.visibility.invalid",
            "workshop.description_marker.invalid",
        ] {
            assert!(
                diagnostics.iter().any(|diagnostic| diagnostic.code == code),
                "missing {code}"
            );
        }
    }

    #[test]
    fn reports_unreadable_metadata_and_formats_all_visibilities() {
        let temporary = tempdir().unwrap();
        let diagnostics = parse(temporary.path()).unwrap_err();
        assert_eq!(diagnostics[0].code, "workshop.unreadable");
        assert_eq!(WorkshopVisibility::FriendsOnly.to_string(), "friendsOnly");
        assert_eq!(WorkshopVisibility::Private.to_string(), "private");
    }

    #[test]
    fn normalizes_tags_and_rejects_invalid_workshop_text() {
        let temporary = tempdir().unwrap();
        let path = temporary.path().join("workshop.txt");
        fs::write(
            &path,
            "version=1\nid=0\ntitle=  Example  \n{{DESCRIPTION}}\ntags= Build 42 ; Multiplayer \nvisibility=private\n",
        )
        .unwrap();
        let metadata = parse(&path).unwrap();
        assert_eq!(metadata.title, "Example");
        assert_eq!(metadata.tags, ["Build 42", "Multiplayer"]);

        fs::write(
            &path,
            format!(
                "version=1\nid=0\ntitle={}\n{{{{DESCRIPTION}}}}\ntags=Build 42;build 42;;bad,tag;{}\nvisibility=private\n",
                "x".repeat(TITLE_MAX_BYTES + 1),
                "x".repeat(TAG_MAX_BYTES + 1),
            ),
        )
        .unwrap();
        let diagnostics = parse(&path).unwrap_err();
        for code in [
            "workshop.title.too_long",
            "workshop.tag.duplicate",
            "workshop.tag.empty",
            "workshop.tag.invalid",
            "workshop.tag.too_long",
        ] {
            assert!(diagnostics.iter().any(|problem| problem.code == code));
        }

        fs::write(
            &path,
            "version=1\nid=0\ntitle=bad\ttitle\n{{DESCRIPTION}}\ntags=Build 42\nvisibility=private\n",
        )
        .unwrap();
        assert!(
            parse(&path)
                .unwrap_err()
                .iter()
                .any(|problem| problem.code == "workshop.title.invalid")
        );
    }
}

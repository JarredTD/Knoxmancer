//! Steam Workshop metadata validation.

use std::fs;
use std::path::Path;

use super::Diagnostic;

/// Placeholder replaced with rendered Workshop description lines.
const DESCRIPTION_MARKER: &str = "{{DESCRIPTION}}";

/// Validates the supported `workshop.txt` fields and description marker.
pub(super) fn validate(path: &Path) -> Vec<Diagnostic> {
    let source = match fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => {
            return vec![Diagnostic::at(
                "workshop.unreadable",
                path,
                error.to_string(),
            )];
        }
    };
    let mut diagnostics = Vec::new();
    let entries: Vec<_> = source
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| match line.split_once('=') {
            Some((key, value)) if !key.is_empty() => Some((key, value)),
            _ if line == DESCRIPTION_MARKER => None,
            _ => {
                diagnostics.push(Diagnostic::at(
                    "workshop.line.invalid",
                    path,
                    format!("invalid metadata line: {line}"),
                ));
                None
            }
        })
        .collect();

    require_exact(&entries, "version", "1", path, &mut diagnostics);
    require_numeric_id(&entries, path, &mut diagnostics);
    require_nonempty(&entries, "title", path, &mut diagnostics);
    require_nonempty(&entries, "tags", path, &mut diagnostics);
    require_visibility(&entries, path, &mut diagnostics);
    if source.matches(DESCRIPTION_MARKER).count() != 1 {
        diagnostics.push(Diagnostic::at(
            "workshop.description_marker.invalid",
            path,
            format!("{DESCRIPTION_MARKER} must appear exactly once"),
        ));
    }
    diagnostics
}

/// Collects all values assigned to one Workshop metadata key.
fn values<'a>(entries: &'a [(&str, &str)], key: &str) -> Vec<&'a str> {
    entries
        .iter()
        .filter_map(|(candidate, value)| (*candidate == key).then_some(*value))
        .collect()
}

/// Requires exactly one field with a prescribed value.
fn require_exact(
    entries: &[(&str, &str)],
    key: &str,
    expected: &str,
    path: &Path,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match values(entries, key).as_slice() {
        [value] if *value == expected => {}
        [value] => diagnostics.push(Diagnostic::at(
            "workshop.value.invalid",
            path,
            format!("{key} must be {expected}, found {value}"),
        )),
        [] => diagnostics.push(Diagnostic::at(
            "workshop.field.missing",
            path,
            format!("{key} is required"),
        )),
        _ => diagnostics.push(Diagnostic::at(
            "workshop.field.duplicate",
            path,
            format!("{key} must appear exactly once"),
        )),
    }
}

/// Requires exactly one unsigned numeric Workshop identifier.
fn require_numeric_id(entries: &[(&str, &str)], path: &Path, diagnostics: &mut Vec<Diagnostic>) {
    match values(entries, "id").as_slice() {
        [value] if value.parse::<u64>().is_ok() => {}
        [value] => diagnostics.push(Diagnostic::at(
            "workshop.id.invalid",
            path,
            format!("id must be an unsigned integer, found {value}"),
        )),
        [] => diagnostics.push(Diagnostic::at(
            "workshop.field.missing",
            path,
            "id is required",
        )),
        _ => diagnostics.push(Diagnostic::at(
            "workshop.field.duplicate",
            path,
            "id must appear exactly once",
        )),
    }
}

/// Requires exactly one non-empty metadata field.
fn require_nonempty(
    entries: &[(&str, &str)],
    key: &str,
    path: &Path,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match values(entries, key).as_slice() {
        [value] if !value.trim().is_empty() => {}
        [_] => diagnostics.push(Diagnostic::at(
            "workshop.value.empty",
            path,
            format!("{key} must not be empty"),
        )),
        [] => diagnostics.push(Diagnostic::at(
            "workshop.field.missing",
            path,
            format!("{key} is required"),
        )),
        _ => diagnostics.push(Diagnostic::at(
            "workshop.field.duplicate",
            path,
            format!("{key} must appear exactly once"),
        )),
    }
}

/// Requires exactly one supported Workshop visibility value.
fn require_visibility(entries: &[(&str, &str)], path: &Path, diagnostics: &mut Vec<Diagnostic>) {
    match values(entries, "visibility").as_slice() {
        ["public" | "friendsOnly" | "private" | "unlisted"] => {}
        [value] => diagnostics.push(Diagnostic::at(
            "workshop.visibility.invalid",
            path,
            format!("unsupported visibility: {value}"),
        )),
        [] => diagnostics.push(Diagnostic::at(
            "workshop.field.missing",
            path,
            "visibility is required",
        )),
        _ => diagnostics.push(Diagnostic::at(
            "workshop.field.duplicate",
            path,
            "visibility must appear exactly once",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn accepts_valid_metadata() {
        let temporary = tempdir().unwrap();
        let path = temporary.path().join("workshop.txt");
        fs::write(
            &path,
            "version=1\nid=0\ntitle=Example\n{{DESCRIPTION}}\ntags=Build 42\nvisibility=unlisted\n",
        )
        .unwrap();
        assert!(validate(&path).is_empty());
    }

    #[test]
    fn reports_structural_and_field_problems() {
        let temporary = tempdir().unwrap();
        let path = temporary.path().join("workshop.txt");
        fs::write(
            &path,
            "broken\nversion=2\nid=nope\ntitle=\ntitle=again\ntags=\nvisibility=hidden\n",
        )
        .unwrap();
        let diagnostics = validate(&path);
        for code in [
            "workshop.line.invalid",
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
}

//! Project Zomboid `mod.info` metadata parsing.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use regex::Regex;
use serde::Serialize;

use crate::error::{Error, Result};

#[derive(Debug, Clone, Serialize)]
pub struct ModMetadata {
    pub name: String,
    pub id: String,
    pub version: String,
    pub build: String,
}

pub fn read(path: &Path, build: &str) -> Result<ModMetadata> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn reads_mod_metadata() {
        let temporary = tempdir().unwrap();
        let path = temporary.path().join("mod.info");
        fs::write(&path, "name=Example\nid=Example\nmodversion=1.2.3\n").unwrap();
        let metadata = read(&path, "42").unwrap();
        assert_eq!(metadata.id, "Example");
        assert_eq!(metadata.version, "1.2.3");
    }

    #[test]
    fn rejects_missing_and_invalid_fields() {
        let temporary = tempdir().unwrap();
        let path = temporary.path().join("mod.info");
        assert!(read(&path, "42").is_err());
        fs::write(&path, "name=Example\nid=bad-id\nmodversion=1.0.0\n").unwrap();
        assert!(read(&path, "42").is_err());
        fs::write(&path, "name=Example\nid=Example\nmodversion=1.2\n").unwrap();
        assert!(read(&path, "42").is_err());
        fs::write(&path, "name=Example\nid=Example\n").unwrap();
        assert!(read(&path, "42").is_err());
    }
}

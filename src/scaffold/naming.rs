//! Scaffold naming derivation and metadata-safe text validation.

use crate::error::{Error, Result};

pub(super) fn display_name(slug: &str) -> String {
    slug.split(['-', '_'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut characters = part.chars();
            match characters.next() {
                Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub(super) fn mod_id(slug: &str) -> String {
    display_name(slug).replace(' ', "")
}

pub(super) fn validate_id(id: &str) -> Result<()> {
    if id.is_empty()
        || !id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return Err(Error::project(
            "mod ID must contain only ASCII letters, digits, and underscores",
        ));
    }
    Ok(())
}

pub(super) fn validate_text(field: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(Error::project(format!("{field} must not be empty")));
    }
    if value.chars().any(char::is_control) {
        return Err(Error::project(format!(
            "{field} must not contain control characters"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_human_and_game_names() {
        assert_eq!(display_name("connected-storage"), "Connected Storage");
        assert_eq!(mod_id("connected-storage"), "ConnectedStorage");
    }

    #[test]
    fn rejects_unsafe_metadata_text() {
        assert!(validate_text("mod name", "").is_err());
        assert!(validate_text("mod name", "bad\nname").is_err());
        assert!(validate_text("author", "valid author").is_ok());
        assert!(validate_id("").is_err());
        assert!(validate_id("valid_ID2").is_ok());
    }
}

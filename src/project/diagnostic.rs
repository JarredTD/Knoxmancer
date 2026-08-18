//! Project validation diagnostics.

use std::fmt;
use std::path::PathBuf;

/// A project validation problem with a stable internal rule identifier.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    /// Stable identifier for the validation rule.
    pub code: &'static str,
    /// File or directory associated with the problem, when applicable.
    pub path: Option<PathBuf>,
    /// Human-readable explanation.
    pub message: String,
}

impl Diagnostic {
    /// Creates a diagnostic without an associated path.
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            path: None,
            message: message.into(),
        }
    }

    /// Creates a diagnostic associated with a path.
    pub fn at(code: &'static str, path: impl Into<PathBuf>, message: impl Into<String>) -> Self {
        Self {
            code,
            path: Some(path.into()),
            message: message.into(),
        }
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(path) = &self.path {
            write!(
                formatter,
                "[{}] {}: {}",
                self.code,
                path.display(),
                self.message
            )
        } else {
            write!(formatter, "[{}] {}", self.code, self.message)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_diagnostics_with_and_without_paths() {
        assert_eq!(
            Diagnostic::new("test", "message").to_string(),
            "[test] message"
        );
        assert_eq!(
            Diagnostic::at("test", "mod.info", "message").to_string(),
            "[test] mod.info: message"
        );
    }
}

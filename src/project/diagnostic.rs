//! Structured validation diagnostics.

use std::fmt;
use std::path::PathBuf;

use serde::Serialize;

/// A machine-readable project validation problem.
#[derive(Debug, Clone, Serialize)]
pub struct Diagnostic {
    /// Stable identifier for the validation rule.
    pub code: &'static str,
    /// File or directory associated with the problem, when applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
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
            write!(formatter, "{}: {}", path.display(), self.message)
        } else {
            formatter.write_str(&self.message)
        }
    }
}

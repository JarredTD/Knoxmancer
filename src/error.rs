//! Command error categories, diagnostics, and exit-code handling.

use std::fmt;
use std::io;

use crate::project::Diagnostic;

#[derive(Debug)]
/// Error returned by a Knoxmancer command.
pub struct Error {
    /// Broad error category used by structured output.
    kind: ErrorKind,
    /// Human-readable failure description.
    message: String,
    /// Process exit code returned to the caller.
    exit_code: u8,
    /// Underlying failure, when the error adapts another error type.
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

#[derive(Debug, Clone, Copy)]
/// Broad command failure category.
pub enum ErrorKind {
    /// Invalid command-line usage or an explicit help/version response.
    Usage,
    /// Invalid project configuration or unsafe project operation.
    Project,
    /// One or more project inputs failed validation.
    Validation,
    /// A filesystem or environment operation failed.
    Io,
}

/// Result type used throughout Knoxmancer.
pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    /// Converts a Clap response into a Knoxmancer error while preserving its exit code.
    pub fn usage(error: clap::Error) -> Self {
        let message = error.to_string();
        let exit_code = error.exit_code().try_into().unwrap_or(2);
        Self {
            kind: ErrorKind::Usage,
            message,
            exit_code,
            source: Some(Box::new(error)),
        }
    }

    /// Creates a project-configuration error.
    pub fn project(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Project, message)
    }

    /// Creates an unstructured validation error.
    pub fn validation(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Validation, message)
    }

    /// Creates a validation error containing machine-readable diagnostics.
    pub fn validation_diagnostics(diagnostics: &[Diagnostic]) -> Self {
        let message = diagnostics
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        Self::new(ErrorKind::Validation, message)
    }

    /// Converts an I/O error into a Knoxmancer error.
    pub fn io(error: io::Error) -> Self {
        Self {
            kind: ErrorKind::Io,
            message: error.to_string(),
            exit_code: 1,
            source: Some(Box::new(error)),
        }
    }

    /// Creates an error of the supplied kind with exit code one.
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            exit_code: 1,
            source: None,
        }
    }

    /// Returns the broad error category.
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    /// Returns the human-readable error message.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns the process exit code associated with the error.
    pub fn exit_code(&self) -> u8 {
        self.exit_code
    }
}

impl ErrorKind {
    /// Returns the stable machine-readable category name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Usage => "usage",
            Self::Project => "project",
            Self::Validation => "validation",
            Self::Io => "io",
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_deref()
            .map(|source| source as &(dyn std::error::Error + 'static))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exposes_every_stable_error_kind() {
        assert_eq!(ErrorKind::Usage.as_str(), "usage");
        assert_eq!(ErrorKind::Project.as_str(), "project");
        assert_eq!(ErrorKind::Validation.as_str(), "validation");
        assert_eq!(ErrorKind::Io.as_str(), "io");
    }

    #[test]
    fn preserves_adapted_error_sources() {
        let error = Error::io(io::Error::new(io::ErrorKind::NotFound, "missing"));
        assert_eq!(
            std::error::Error::source(&error).unwrap().to_string(),
            "missing"
        );
        assert!(std::error::Error::source(&Error::project("invalid")).is_none());
    }
}

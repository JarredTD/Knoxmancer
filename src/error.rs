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
    /// Structured validation details, when applicable.
    diagnostics: Option<Vec<Diagnostic>>,
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
        Self {
            kind: ErrorKind::Usage,
            message: error.to_string(),
            exit_code: error.exit_code().try_into().unwrap_or(2),
            diagnostics: None,
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
    pub fn validation_diagnostics(diagnostics: Vec<Diagnostic>) -> Self {
        let message = diagnostics
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n");
        Self {
            kind: ErrorKind::Validation,
            message,
            exit_code: 1,
            diagnostics: Some(diagnostics),
        }
    }

    /// Converts an I/O error into a Knoxmancer error.
    pub fn io(error: io::Error) -> Self {
        Self::new(ErrorKind::Io, error.to_string())
    }

    /// Creates an error of the supplied kind with exit code one.
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            exit_code: 1,
            diagnostics: None,
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

    /// Returns structured validation diagnostics when available.
    pub fn diagnostics(&self) -> Option<&[Diagnostic]> {
        self.diagnostics.as_deref()
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

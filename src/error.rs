use std::fmt;
use std::io;

use crate::project::Diagnostic;

#[derive(Debug)]
pub struct Error {
    kind: ErrorKind,
    message: String,
    exit_code: u8,
    diagnostics: Option<Vec<Diagnostic>>,
}

#[derive(Debug, Clone, Copy)]
pub enum ErrorKind {
    Usage,
    Project,
    Validation,
    Tool,
    Io,
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn usage(error: clap::Error) -> Self {
        Self {
            kind: ErrorKind::Usage,
            message: error.to_string(),
            exit_code: error.exit_code().try_into().unwrap_or(2),
            diagnostics: None,
        }
    }

    pub fn project(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Project, message)
    }

    pub fn validation(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Validation, message)
    }

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

    pub fn tool(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Tool, message)
    }

    pub fn io(error: io::Error) -> Self {
        Self::new(ErrorKind::Io, error.to_string())
    }

    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            exit_code: 1,
            diagnostics: None,
        }
    }

    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn exit_code(&self) -> u8 {
        self.exit_code
    }

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

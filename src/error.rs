use std::fmt;
use std::io;

#[derive(Debug)]
pub struct Error {
    kind: ErrorKind,
    message: String,
}

#[derive(Debug, Clone, Copy)]
pub enum ErrorKind {
    Usage,
    Project,
    Validation,
    Tool,
    Io,
    NotImplemented,
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn usage(error: clap::Error) -> Self {
        Self::new(ErrorKind::Usage, error.to_string())
    }

    pub fn project(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Project, message)
    }

    pub fn validation(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Validation, message)
    }

    pub fn tool(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::Tool, message)
    }

    pub fn io(error: io::Error) -> Self {
        Self::new(ErrorKind::Io, error.to_string())
    }

    pub fn not_implemented(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::NotImplemented, message)
    }

    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn exit_code(&self) -> u8 {
        match self.kind {
            ErrorKind::Usage => 2,
            _ => 1,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

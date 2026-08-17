use serde::Serialize;

use crate::cli::{OutputFormat, OutputOptions};
use crate::error::{Error, ErrorKind};

pub const JSON_SCHEMA_VERSION: u32 = 1;

pub struct Reporter {
    options: OutputOptions,
}

#[derive(Serialize)]
struct ErrorEvent<'a> {
    schema_version: u32,
    status: &'static str,
    kind: &'static str,
    message: &'a str,
}

impl Reporter {
    pub fn new(options: OutputOptions) -> Self {
        Self { options }
    }

    pub fn status(&self, message: &str) {
        if self.options.quiet {
            return;
        }
        match self.options.format {
            OutputFormat::Human => eprintln!("{message}"),
            OutputFormat::Json => println!(
                "{}",
                serde_json::json!({
                    "schema_version": JSON_SCHEMA_VERSION,
                    "status": "ok",
                    "message": message,
                })
            ),
        }
    }

    pub fn verbose(&self, message: &str) {
        if self.options.verbose {
            self.status(message);
        }
    }

    pub fn error(&self, error: &Error) {
        match self.options.format {
            OutputFormat::Human => eprintln!("error: {}", error.message()),
            OutputFormat::Json => println!(
                "{}",
                serde_json::to_string(&ErrorEvent {
                    schema_version: JSON_SCHEMA_VERSION,
                    status: "error",
                    kind: kind_name(error.kind()),
                    message: error.message(),
                })
                .expect("error event is serializable")
            ),
        }
    }
}

fn kind_name(kind: ErrorKind) -> &'static str {
    match kind {
        ErrorKind::Usage => "usage",
        ErrorKind::Project => "project",
        ErrorKind::Validation => "validation",
        ErrorKind::Tool => "tool",
        ErrorKind::Io => "io",
        ErrorKind::NotImplemented => "not_implemented",
    }
}

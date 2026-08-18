//! Human-readable and NDJSON output rendering.

use serde::Serialize;
use std::io::IsTerminal;

use super::args::{ColorChoice, OutputFormat, OutputOptions};
use crate::error::{Error, ErrorKind};
use crate::project::Diagnostic;

/// Schema version emitted in structured command events.
pub const JSON_SCHEMA_VERSION: u32 = 1;

/// Writes command status and errors in the selected output format.
pub struct Reporter {
    /// Global output policy parsed from the command line.
    options: OutputOptions,
}

#[derive(Serialize)]
/// Borrowed structured representation of a command failure.
struct ErrorEvent<'a> {
    /// Structured-output schema version.
    schema_version: u32,
    /// Machine-readable event status.
    status: &'static str,
    /// Machine-readable error category.
    kind: &'static str,
    /// Human-readable failure description.
    message: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Detailed validation problems when available.
    diagnostics: Option<&'a [Diagnostic]>,
}

impl Reporter {
    /// Creates a reporter using the supplied global output policy.
    pub fn new(options: OutputOptions) -> Self {
        Self { options }
    }

    /// Emits a successful status event unless quiet output is enabled.
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

    /// Emits a status event only when verbose output is enabled.
    pub fn verbose(&self, message: &str) {
        if self.options.verbose {
            self.status(message);
        }
    }

    /// Emits a command failure in human-readable or structured form.
    pub fn error(&self, error: &Error) {
        match self.options.format {
            OutputFormat::Human => {
                let color = match self.options.color {
                    ColorChoice::Always => true,
                    ColorChoice::Never => false,
                    ColorChoice::Auto => std::io::stderr().is_terminal(),
                };
                if color {
                    eprintln!("\x1b[31merror:\x1b[0m {}", error.message());
                } else {
                    eprintln!("error: {}", error.message());
                }
            }
            OutputFormat::Json => println!(
                "{}",
                serde_json::to_string(&ErrorEvent {
                    schema_version: JSON_SCHEMA_VERSION,
                    status: "error",
                    kind: kind_name(error.kind()),
                    message: error.message(),
                    diagnostics: error.diagnostics(),
                })
                .expect("error event is serializable")
            ),
        }
    }
}

/// Returns the stable structured-output name for an error category.
fn kind_name(kind: ErrorKind) -> &'static str {
    match kind {
        ErrorKind::Usage => "usage",
        ErrorKind::Project => "project",
        ErrorKind::Validation => "validation",
        ErrorKind::Io => "io",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_every_error_kind() {
        assert_eq!(kind_name(ErrorKind::Usage), "usage");
        assert_eq!(kind_name(ErrorKind::Project), "project");
        assert_eq!(kind_name(ErrorKind::Validation), "validation");
        assert_eq!(kind_name(ErrorKind::Io), "io");
    }
}

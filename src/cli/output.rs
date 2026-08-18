//! Stable human-readable and JSON Lines terminal output rendering.

use std::io::{IsTerminal, Write};
use std::path::Path;

use serde::Serialize;

use super::args::{ColorChoice, OutputFormat, OutputOptions};
use crate::error::Error;

/// Writes command data to stdout and diagnostics to stderr.
pub struct Reporter {
    /// Global output policy parsed from the command line.
    options: OutputOptions,
}

impl Reporter {
    /// Creates a reporter using the supplied global output policy.
    pub fn new(options: OutputOptions) -> Self {
        Self { options }
    }

    /// Emits a successful status event to stdout unless quiet output is enabled.
    pub fn status(&self, message: &str) {
        if self.options.quiet {
            return;
        }
        match self.options.format {
            OutputFormat::Human => write_human(std::io::stdout(), message),
            OutputFormat::Json => write_json(
                std::io::stdout(),
                &MessageEvent {
                    event_type: "status",
                    message,
                },
            ),
        }
    }

    /// Emits one resolved path as requested command data on stdout.
    pub fn path(&self, name: &'static str, label: &str, path: &Path) {
        if self.options.quiet {
            return;
        }
        match self.options.format {
            OutputFormat::Human => {
                write_human(std::io::stdout(), &format!("{label}: {}", path.display()));
            }
            OutputFormat::Json => write_json(
                std::io::stdout(),
                &PathEvent {
                    event_type: "path",
                    name,
                    path: &path.display().to_string(),
                },
            ),
        }
    }

    /// Emits a non-fatal warning to stderr even when quiet output is enabled.
    pub fn warning(&self, message: &str) {
        match self.options.format {
            OutputFormat::Human => write_human(std::io::stderr(), &format!("warning: {message}")),
            OutputFormat::Json => write_json(
                std::io::stderr(),
                &MessageEvent {
                    event_type: "warning",
                    message,
                },
            ),
        }
    }

    /// Emits a command failure to stderr.
    pub fn error(&self, error: &Error) {
        if self.options.format == OutputFormat::Json {
            Self::json_error(error);
            return;
        }

        let color = match self.options.color {
            ColorChoice::Always => true,
            ColorChoice::Never => false,
            ColorChoice::Auto => std::io::stderr().is_terminal(),
        };
        if color {
            write_human(
                std::io::stderr(),
                &format!("\x1b[31merror:\x1b[0m {}", error.message()),
            );
        } else {
            write_human(std::io::stderr(), &format!("error: {}", error.message()));
        }
    }

    /// Emits a JSON error without requiring fully parsed output options.
    pub fn json_error(error: &Error) {
        write_json(
            std::io::stderr(),
            &ErrorEvent {
                event_type: "error",
                kind: error.kind().as_str(),
                message: error.message(),
                exit_code: error.exit_code(),
            },
        );
    }

    /// Emits a human-readable Clap response while tolerating a closed output pipe.
    pub fn usage(error: &Error) {
        if error.exit_code() == 0 {
            write_raw(std::io::stdout(), error.message());
        } else {
            write_raw(std::io::stderr(), error.message());
        }
    }
}

/// A JSON status or warning event.
#[derive(Serialize)]
struct MessageEvent<'a> {
    /// Event discriminator.
    #[serde(rename = "type")]
    event_type: &'static str,
    /// Human-readable event content.
    message: &'a str,
}

/// A JSON resolved-path event.
#[derive(Serialize)]
struct PathEvent<'a> {
    /// Event discriminator.
    #[serde(rename = "type")]
    event_type: &'static str,
    /// Stable path category.
    name: &'static str,
    /// Resolved path encoded losslessly when it is valid Unicode.
    path: &'a str,
}

/// A JSON command-failure event.
#[derive(Serialize)]
struct ErrorEvent<'a> {
    /// Event discriminator.
    #[serde(rename = "type")]
    event_type: &'static str,
    /// Stable broad error category.
    kind: &'static str,
    /// Human-readable failure explanation.
    message: &'a str,
    /// Process exit code returned by the command.
    exit_code: u8,
}

/// Writes one plain-text line while tolerating a closed output pipe.
fn write_human(stream: impl Write, message: &str) {
    let mut stream = stream;
    let _ = writeln!(stream, "{message}");
}

/// Writes text verbatim while tolerating a closed output pipe.
fn write_raw(stream: impl Write, message: &str) {
    let mut stream = stream;
    let _ = stream.write_all(message.as_bytes());
}

/// Writes one JSON object and newline while tolerating a closed output pipe.
fn write_json(stream: impl Write, value: &impl Serialize) {
    let mut stream = stream;
    if serde_json::to_writer(&mut stream, value).is_ok() {
        let _ = writeln!(stream);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_human_and_json_events() {
        for format in [OutputFormat::Human, OutputFormat::Json] {
            let reporter = Reporter::new(OutputOptions {
                quiet: true,
                color: ColorChoice::Never,
                format,
            });
            reporter.status("hidden status");
            reporter.warning("visible warning");
            reporter.path("test", "Test", Path::new("path"));
            reporter.error(&Error::validation("invalid"));
        }
    }
}

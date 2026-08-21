//! Stable human-readable and JSON Lines terminal output rendering.

use std::io::{IsTerminal, Write};
use std::path::Path;

use serde::Serialize;

use super::args::{ColorChoice, OutputFormat, OutputOptions};
use crate::build::{LiveOperation, LiveStatus};
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

    /// Emits one discovered installed mod copy.
    pub(crate) fn mod_copy(
        &self,
        source: &'static str,
        label: &str,
        version: Option<&str>,
        current: bool,
        path: &Path,
    ) {
        if self.options.quiet {
            return;
        }
        match self.options.format {
            OutputFormat::Human => write_human(
                std::io::stdout(),
                &format!(
                    "{label}: {}{} ({})",
                    version.unwrap_or("unknown version"),
                    if current {
                        " [current]"
                    } else {
                        " [different]"
                    },
                    path.display()
                ),
            ),
            OutputFormat::Json => write_json(
                std::io::stdout(),
                &ModCopyEvent {
                    event_type: "mod_copy",
                    source,
                    version,
                    current,
                    path: &path.display().to_string(),
                },
            ),
        }
    }

    /// Emits one live-install file operation using its stable structured form.
    pub(crate) fn live_operation(&self, operation: &LiveOperation) {
        let diagnostic = matches!(operation.status, LiveStatus::Failed | LiveStatus::Skipped);
        if self.options.quiet && !diagnostic {
            return;
        }
        match self.options.format {
            OutputFormat::Human => {
                if operation.status == LiveStatus::Unchanged {
                    return;
                }
                let message = format!(
                    "{} {}: {}{}",
                    operation.status.as_str(),
                    operation.action.as_str(),
                    operation.path.display(),
                    operation
                        .message
                        .as_deref()
                        .map(|message| format!(": {message}"))
                        .unwrap_or_default()
                );
                if diagnostic {
                    write_human(std::io::stderr(), &message);
                } else {
                    write_human(std::io::stdout(), &message);
                }
            }
            OutputFormat::Json => {
                let event = FileOperationEvent {
                    event_type: "file_operation",
                    action: operation.action.as_str(),
                    status: operation.status.as_str(),
                    path: &operation.path.display().to_string(),
                    message: operation.message.as_deref(),
                };
                if diagnostic {
                    write_json(std::io::stderr(), &event);
                } else {
                    write_json(std::io::stdout(), &event);
                }
            }
        }
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

/// A JSON event describing one discovered mod copy.
#[derive(Serialize)]
struct ModCopyEvent<'a> {
    /// Event discriminator.
    #[serde(rename = "type")]
    event_type: &'static str,
    /// Stable installation source.
    source: &'static str,
    /// Version declared by the copy, when available.
    version: Option<&'a str>,
    /// Whether the copy matches the project version.
    current: bool,
    /// Root directory of the matching mod.
    path: &'a str,
}

/// A JSON event describing one live-install filesystem operation.
#[derive(Serialize)]
struct FileOperationEvent<'a> {
    /// Event discriminator.
    #[serde(rename = "type")]
    event_type: &'static str,
    /// Stable intended operation.
    action: &'static str,
    /// Stable operation outcome.
    status: &'static str,
    /// Artifact-relative path.
    path: &'a str,
    /// Failure or skip explanation.
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<&'a str>,
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
    use std::path::PathBuf;

    use crate::build::LiveAction;

    use super::*;

    #[test]
    fn renders_human_and_json_events() {
        for format in [OutputFormat::Human, OutputFormat::Json] {
            for quiet in [true, false] {
                let reporter = Reporter::new(OutputOptions {
                    quiet,
                    color: ColorChoice::Never,
                    format,
                });
                reporter.status("status");
                reporter.warning("visible warning");
                reporter.path("test", "Test", Path::new("path"));
                reporter.error(&Error::validation("invalid"));
                reporter.mod_copy("local", "Local", Some("1.0.0"), true, Path::new("mod"));

                for (status, message) in [
                    (LiveStatus::Applied, None),
                    (LiveStatus::Unchanged, None),
                    (LiveStatus::Failed, Some("failed".to_owned())),
                    (LiveStatus::Skipped, Some("skipped".to_owned())),
                ] {
                    reporter.live_operation(&LiveOperation {
                        action: LiveAction::Update,
                        status,
                        path: PathBuf::from("42/file.lua"),
                        message,
                    });
                }
            }
        }
    }
}

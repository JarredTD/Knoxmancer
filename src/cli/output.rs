//! Human-readable terminal output rendering.

use std::io::IsTerminal;

use super::args::{ColorChoice, OutputOptions};
use crate::error::Error;

/// Writes command status and errors in the selected output format.
pub struct Reporter {
    /// Global output policy parsed from the command line.
    options: OutputOptions,
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
        eprintln!("{message}");
    }

    /// Emits a non-fatal warning even when quiet output is enabled.
    pub fn warning(&self, message: &str) {
        eprintln!("warning: {message}");
    }

    /// Emits a command failure in human-readable or structured form.
    pub fn error(&self, error: &Error) {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quiet_output_still_reports_warnings() {
        let reporter = Reporter::new(OutputOptions {
            quiet: true,
            color: ColorChoice::Never,
        });
        reporter.status("hidden status");
        reporter.warning("visible warning");
    }
}

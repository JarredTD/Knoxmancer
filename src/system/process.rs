//! Captured subprocess execution for deterministic CLI output.

use std::path::Path;
use std::process::Command;

use crate::error::{Error, Result};

/// Captured output from a completed subprocess.
#[derive(Debug)]
pub struct ProcessOutput {
    success: bool,
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

impl ProcessOutput {
    /// Whether the process returned a successful exit status.
    pub fn success(&self) -> bool {
        self.success
    }

    /// Describes the exit status for diagnostics.
    pub fn status_description(&self) -> String {
        self.code
            .map_or_else(|| "a signal".to_owned(), |code| code.to_string())
    }

    /// Returns non-empty stdout and stderr lines in emission order.
    pub fn lines(&self) -> Vec<String> {
        self.stdout
            .lines()
            .chain(self.stderr.lines())
            .filter(|line| !line.trim().is_empty())
            .map(|line| line.trim().to_owned())
            .collect()
    }

    /// Returns captured output suitable for inclusion in a failure message.
    pub fn failure_detail(&self) -> String {
        let lines = self.lines();
        if lines.is_empty() {
            String::new()
        } else {
            format!("\n{}", lines.join("\n"))
        }
    }
}

/// Runs a command without allowing it to write directly to the CLI streams.
pub fn run(
    program: &str,
    arguments: &[String],
    current_dir: Option<&Path>,
) -> Result<ProcessOutput> {
    let mut command = Command::new(program);
    command.args(arguments);
    if let Some(directory) = current_dir {
        command.current_dir(directory);
    }
    let output = command
        .output()
        .map_err(|error| Error::tool(format!("could not run {program}: {error}")))?;
    Ok(ProcessOutput {
        success: output.status.success(),
        code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_output_and_exit_status() {
        #[cfg(windows)]
        let (program, arguments) = (
            "cmd",
            vec![
                "/c".to_owned(),
                "echo out & echo err 1>&2 & exit 7".to_owned(),
            ],
        );
        #[cfg(unix)]
        let (program, arguments) = (
            "sh",
            vec![
                "-c".to_owned(),
                "printf 'out\\n'; printf 'err\\n' >&2; exit 7".to_owned(),
            ],
        );

        let output = run(program, &arguments, None).unwrap();
        assert!(!output.success());
        assert_eq!(output.status_description(), "7");
        assert_eq!(output.lines(), ["out", "err"]);
        assert!(output.failure_detail().contains("out"));
    }
}

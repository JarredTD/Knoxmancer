//! Configured project test-command execution.

use std::process::Command;

use crate::error::{Error, Result};
use crate::validation::ValidatedProject;

pub(crate) fn run(validated: &ValidatedProject<'_>) -> Result<()> {
    let project = validated.project;
    let (program, arguments) = project
        .config
        .test
        .command
        .split_first()
        .ok_or_else(|| Error::project("no test.command is configured in knoxmancer.toml"))?;
    let status = Command::new(program)
        .args(arguments)
        .current_dir(&project.root)
        .status()
        .map_err(|error| Error::tool(format!("could not run {program}: {error}")))?;
    if !status.success() {
        return Err(Error::tool(format!(
            "test command exited with {}",
            status
                .code()
                .map_or_else(|| "a signal".to_owned(), |code| code.to_string())
        )));
    }
    Ok(())
}

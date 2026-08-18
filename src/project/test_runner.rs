//! Configured project test-command execution.

use super::validation::ValidatedProject;
use crate::error::{Error, Result};
use crate::system::process;

pub(crate) fn run(validated: &ValidatedProject<'_>) -> Result<Vec<String>> {
    let project = validated.project;
    let (program, arguments) = project
        .config
        .test
        .command
        .split_first()
        .ok_or_else(|| Error::project("no test.command is configured in knoxmancer.toml"))?;
    let output = process::run(program, arguments, Some(&project.root))?;
    if !output.success() {
        return Err(Error::tool(format!(
            "test command exited with {}{}",
            output.status_description(),
            output.failure_detail()
        )));
    }
    Ok(output.lines())
}

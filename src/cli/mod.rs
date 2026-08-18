//! Command-line parsing and presentation.

pub(crate) mod args;
pub(crate) mod output;

pub(crate) use args::{Cli, Command, ConfigArgs, ConfigCommand, ConfigKey, NewArgs};
pub(crate) use output::Reporter;

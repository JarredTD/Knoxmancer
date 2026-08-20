//! Command-line parsing and presentation.

pub(crate) mod args;
pub(crate) mod output;

pub(crate) use args::{
    Cli, Command, CompletionsArgs, ConfigArgs, ConfigCommand, ConfigKey, NewArgs, OpenTarget,
};
pub(crate) use output::Reporter;

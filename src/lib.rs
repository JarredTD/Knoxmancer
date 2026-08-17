pub mod cli;
pub mod config;
pub mod error;
pub mod output;
pub mod scaffold;

use std::ffi::OsString;

use clap::Parser;
use cli::{Cli, Command};
use error::{Error, Result};
use output::Reporter;

pub fn run<I, T>(args: I) -> Result<()>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let cli = Cli::try_parse_from(args).map_err(Error::usage)?;
    let reporter = Reporter::new(cli.output_options());

    match cli.command {
        Command::New(args) => scaffold::new_project(&args, &reporter),
        Command::Init(args) => scaffold::init_project(cli.project.as_deref(), &args, &reporter),
        Command::Doctor(_) | Command::Check(_) | Command::Test(_) => Err(Error::not_implemented(
            "project validation is not available in this build",
        )),
        Command::Build(_) | Command::Install(_) | Command::Package(_) | Command::Clean(_) => Err(
            Error::not_implemented("artifact commands are not available in this build"),
        ),
    }
    .map_err(|error| {
        reporter.error(&error);
        error
    })
}

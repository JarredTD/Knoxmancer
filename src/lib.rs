//! Project Zomboid mod development commands used by the `knoxmancer` and `km` binaries.
#![deny(missing_docs)]
#![forbid(unsafe_code)]

mod artifact;
mod cli;
mod config;
mod error;
mod output;
mod scaffold;
mod validation;

use std::ffi::OsString;

use clap::Parser;
use cli::{Cli, Command};
use error::{Error, Result};
use output::Reporter;

/// Executes Knoxmancer with the supplied command-line arguments and returns its process exit code.
pub fn execute<I, T>(args: I) -> u8
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    match run(args) {
        Ok(()) => 0,
        Err(error) => {
            if matches!(error.kind(), error::ErrorKind::Usage) {
                if error.exit_code() == 0 {
                    print!("{error}");
                } else {
                    eprint!("{error}");
                }
            }
            error.exit_code()
        }
    }
}

fn run<I, T>(args: I) -> Result<()>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let cli = Cli::try_parse_from(args).map_err(Error::usage)?;
    let reporter = Reporter::new(cli.output_options());

    let result = (|| match cli.command {
        Command::New(args) => scaffold::new_project(&args, &reporter),
        Command::Init(args) => scaffold::init_project(cli.project.as_deref(), &args, &reporter),
        Command::Doctor(args) => validation::doctor(&args, &reporter),
        Command::Check(args) => {
            let project = config::Project::discover(cli.project.as_deref())?;
            validation::check(&project, args.release, &reporter).map(|_| ())
        }
        Command::Test(args) => {
            let project = config::Project::discover(cli.project.as_deref())?;
            validation::test(&project, &args, &reporter)
        }
        Command::Build(args) => {
            let project = config::Project::discover(cli.project.as_deref())?;
            artifact::build(&project, args.release, &reporter).map(|_| ())
        }
        Command::Install(args) => {
            let project = config::Project::discover(cli.project.as_deref())?;
            artifact::install(&project, &args, &reporter).map(|_| ())
        }
        Command::Package(args) => {
            let project = config::Project::discover(cli.project.as_deref())?;
            artifact::package(&project, &args, &reporter).map(|_| ())
        }
        Command::Clean(args) => {
            let project = config::Project::discover(cli.project.as_deref())?;
            artifact::clean(&project, &args, &reporter)
        }
    })();
    result.inspect_err(|error| {
        reporter.error(error);
    })
}

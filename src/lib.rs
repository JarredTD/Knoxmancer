pub mod artifact;
pub mod cli;
pub mod config;
pub mod error;
pub mod output;
pub mod scaffold;
pub mod validation;

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
    }
    .inspect_err(|error| {
        reporter.error(error);
    })
}

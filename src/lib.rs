//! Project Zomboid mod development commands used by the `knoxmancer` and `km` binaries.
#![deny(missing_docs)]
#![deny(clippy::missing_docs_in_private_items)]
#![forbid(unsafe_code)]

mod app;
mod build;
mod cli;
mod error;
mod project;
mod scaffold;
mod system;
mod workshop;

use std::ffi::OsString;

use clap::Parser;
use cli::{Cli, Reporter};
use error::{Error, Result};

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

/// Parses arguments, constructs an output reporter, and dispatches one command.
fn run<I, T>(args: I) -> Result<()>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let cli = Cli::try_parse_from(args).map_err(Error::usage)?;
    let reporter = Reporter::new(cli.output_options());

    let result = app::run(cli, &reporter);
    result.inspect_err(|error| {
        reporter.error(error);
    })
}

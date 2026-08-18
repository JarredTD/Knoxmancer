//! Project Zomboid mod development commands used by the `knoxmancer` and `km` binaries.
#![deny(missing_docs)]
#![forbid(unsafe_code)]

mod app;
mod artifact;
mod cli;
mod config;
mod diagnostic;
mod environment;
mod error;
mod filesystem;
mod layout;
mod metadata;
mod minify;
mod output;
mod preview;
mod process;
mod scaffold;
mod templates;
mod test_runner;
mod validation;
mod workshop;

use std::ffi::OsString;

use clap::Parser;
use cli::Cli;
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

    let result = app::run(cli, &reporter);
    result.inspect_err(|error| {
        reporter.error(error);
    })
}

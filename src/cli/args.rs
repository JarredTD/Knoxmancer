//! Command-line argument definitions.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(
    name = "knoxmancer",
    bin_name = "knoxmancer",
    version,
    about = "Project Zomboid mod development CLI"
)]
pub struct Cli {
    /// Starts project discovery from this path instead of the current directory.
    #[arg(long, global = true, value_name = "PATH")]
    pub project: Option<PathBuf>,

    /// Suppresses non-error output.
    #[arg(short, long, global = true, conflicts_with = "verbose")]
    pub quiet: bool,

    /// Shows additional filesystem and tool details.
    #[arg(short, long, global = true, conflicts_with = "quiet")]
    pub verbose: bool,

    /// Controls ANSI color in human-readable diagnostics.
    #[arg(long, global = true, value_enum, default_value_t = ColorChoice::Auto)]
    pub color: ColorChoice,

    /// Selects human-readable or versioned newline-delimited JSON output.
    #[arg(long, global = true, value_enum, default_value_t = OutputFormat::Human)]
    pub format: OutputFormat,

    #[command(subcommand)]
    pub command: Command,
}

impl Cli {
    pub fn output_options(&self) -> OutputOptions {
        OutputOptions {
            quiet: self.quiet,
            verbose: self.verbose,
            color: self.color,
            format: self.format,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct OutputOptions {
    pub quiet: bool,
    pub verbose: bool,
    pub color: ColorChoice,
    pub format: OutputFormat,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ColorChoice {
    Auto,
    Always,
    Never,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum OutputFormat {
    Human,
    Json,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Creates a complete Project Zomboid mod project.
    New(NewArgs),
    /// Adopts an existing mod project without rewriting game metadata.
    Init(InitArgs),
    /// Reports local game paths and external tool availability.
    Doctor(DoctorArgs),
    /// Validates project structure, metadata, and assets.
    Check(CheckArgs),
    /// Runs the test command configured by the project.
    Test(TestArgs),
    /// Creates a development or release artifact.
    Build(BuildArgs),
    /// Builds and atomically installs the mod locally.
    Install(InstallArgs),
    /// Creates a verified Steam Workshop directory tree.
    Package(PackageArgs),
    /// Removes Knoxmancer-generated artifacts.
    Clean(CleanArgs),
}

#[derive(Debug, Args)]
pub struct NewArgs {
    pub directory: PathBuf,
    #[arg(long)]
    pub name: Option<String>,
    #[arg(long)]
    pub id: Option<String>,
    #[arg(long)]
    pub author: Option<String>,
    #[arg(long, default_value = "42", value_parser = ["42"])]
    pub build: String,
}

#[derive(Debug, Args)]
pub struct InitArgs {
    #[arg(long)]
    pub force: bool,
}

#[derive(Debug, Args, Default)]
pub struct DoctorArgs {}

#[derive(Debug, Args, Default)]
pub struct CheckArgs {
    #[arg(long)]
    pub release: bool,
}

#[derive(Debug, Args, Default)]
pub struct TestArgs {}

#[derive(Debug, Args, Default)]
pub struct BuildArgs {
    #[arg(long)]
    pub release: bool,
}

#[derive(Debug, Args, Default)]
pub struct InstallArgs {
    #[arg(long)]
    pub release: bool,
    #[arg(long, value_name = "PATH")]
    pub root: Option<PathBuf>,
}

#[derive(Debug, Args, Default)]
pub struct PackageArgs {}

#[derive(Debug, Args, Default)]
pub struct CleanArgs {}

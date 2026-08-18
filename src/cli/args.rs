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
/// Parsed Knoxmancer command line.
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
    /// Operation selected by the user.
    pub command: Command,
}

impl Cli {
    /// Extracts the global output controls used by the reporter.
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
/// Global output controls shared by all commands.
pub struct OutputOptions {
    /// Suppresses successful status messages.
    pub quiet: bool,
    /// Enables additional operational detail.
    pub verbose: bool,
    /// Controls ANSI color in human-readable errors.
    pub color: ColorChoice,
    /// Selects human-readable or structured output.
    pub format: OutputFormat,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
/// Policy for ANSI color in human-readable output.
pub enum ColorChoice {
    /// Uses color only when standard error is a terminal.
    Auto,
    /// Always emits ANSI color escapes.
    Always,
    /// Never emits ANSI color escapes.
    Never,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
/// Serialization format for command events.
pub enum OutputFormat {
    /// Concise messages intended for a terminal.
    Human,
    /// Versioned newline-delimited JSON events.
    Json,
}

#[derive(Debug, Subcommand)]
/// Supported Knoxmancer operations.
pub enum Command {
    /// Creates a complete Project Zomboid mod project.
    New(NewArgs),
    /// Adopts an existing mod project without rewriting game metadata.
    Init(InitArgs),
    /// Reports Git availability and the local mods path.
    Doctor(DoctorArgs),
    /// Validates project structure, metadata, and assets.
    Check(CheckArgs),
    /// Creates a development or release artifact.
    Build(BuildArgs),
    /// Builds and atomically installs the mod locally.
    Install(InstallArgs),
    /// Creates and optionally stages a verified Steam Workshop directory tree.
    Package(PackageArgs),
    /// Removes Knoxmancer-generated artifacts.
    Clean(CleanArgs),
}

#[derive(Debug, Args)]
/// Arguments for creating a new mod project.
pub struct NewArgs {
    /// Empty destination directory to create or populate.
    pub directory: PathBuf,
    #[arg(long)]
    /// Human-readable mod name; derived from the directory when omitted.
    pub name: Option<String>,
    #[arg(long)]
    /// Game-facing mod identifier; derived from the directory when omitted.
    pub id: Option<String>,
    #[arg(long)]
    /// Mod author; derived from the local environment when omitted.
    pub author: Option<String>,
    #[arg(long, default_value = "42", value_parser = ["42"])]
    /// Project Zomboid build directory to scaffold.
    pub build: String,
}

#[derive(Debug, Args)]
/// Arguments for adopting an existing mod project.
pub struct InitArgs {
    #[arg(long)]
    /// Replaces an existing Knoxmancer manifest.
    pub force: bool,
}

#[derive(Debug, Args, Default)]
/// Arguments for environment diagnostics.
pub struct DoctorArgs {}

#[derive(Debug, Args, Default)]
/// Arguments for project validation.
pub struct CheckArgs {
    #[arg(long)]
    /// Enables checks required for publishing artifacts.
    pub release: bool,
}

#[derive(Debug, Args, Default)]
/// Arguments for artifact construction.
pub struct BuildArgs {
    #[arg(long)]
    /// Builds with publishing validation enabled.
    pub release: bool,
}

#[derive(Debug, Args, Default)]
/// Arguments for local mod installation.
pub struct InstallArgs {
    #[arg(long)]
    /// Installs an artifact built with publishing validation.
    pub release: bool,
    #[arg(long, value_name = "PATH")]
    /// Overrides the default Project Zomboid mods root.
    pub root: Option<PathBuf>,
}

#[derive(Debug, Args, Default)]
/// Arguments for Workshop package construction and uploader staging.
pub struct PackageArgs {
    #[arg(long)]
    /// Stages the package in the installed Project Zomboid uploader directory.
    pub stage: bool,
    #[arg(long, value_name = "PATH", requires = "stage")]
    /// Overrides the detected Project Zomboid uploader mods root.
    pub root: Option<PathBuf>,
}

#[derive(Debug, Args, Default)]
/// Arguments for generated-artifact cleanup.
pub struct CleanArgs {}

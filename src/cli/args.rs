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
    #[arg(short, long, global = true)]
    pub quiet: bool,

    /// Controls ANSI color in human-readable diagnostics.
    #[arg(long, global = true, value_enum, default_value_t = ColorChoice::Auto)]
    pub color: ColorChoice,

    /// Selects human-readable output or newline-delimited JSON events.
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
    /// Controls ANSI color in human-readable errors.
    pub color: ColorChoice,
    /// Selects the serialized output contract.
    pub format: OutputFormat,
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
/// Stable output serialization format.
pub enum OutputFormat {
    /// Writes requested data and status messages as plain text.
    Human,
    /// Writes one JSON object per event.
    Json,
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

#[derive(Debug, Subcommand)]
/// Supported Knoxmancer operations.
pub enum Command {
    /// Creates a complete Project Zomboid mod project.
    New(NewArgs),
    /// Adopts an existing mod project without rewriting game metadata.
    Init(InitArgs),
    /// Reports resolved artifact, installation, and staging paths.
    Paths(PathsArgs),
    /// Validates the project for local builds.
    Check(CheckArgs),
    /// Creates a playable artifact under the output directory.
    Build(BuildArgs),
    /// Builds and installs the mod for local play.
    Install(InstallArgs),
    /// Creates a verified Workshop upload project under the output directory.
    Package(PackageArgs),
    /// Packages and stages the mod for Zomboid's Workshop uploader.
    Stage(StageArgs),
    /// Removes Knoxmancer-generated artifacts.
    Clean(CleanArgs),
    /// Reads or updates machine-specific user defaults.
    Config(ConfigArgs),
    /// Generates a shell completion script.
    Completions(CompletionsArgs),
    /// Checks project and environment readiness without writing artifacts.
    Doctor(DoctorArgs),
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
    /// Mod author; uses `Your Name` when omitted.
    pub author: Option<String>,
}

#[derive(Debug, Args)]
/// Arguments for adopting an existing mod project.
pub struct InitArgs {
    #[arg(long)]
    /// Replaces an existing Knoxmancer manifest.
    pub force: bool,
}

#[derive(Debug, Args, Default)]
/// Arguments for resolved project paths.
pub struct PathsArgs {}

#[derive(Debug, Args, Default)]
/// Arguments for project validation.
pub struct CheckArgs {
    /// Also validates Workshop metadata, assets, and package inputs.
    #[arg(long)]
    pub workshop: bool,
}

#[derive(Debug, Args, Default)]
/// Arguments for artifact construction.
pub struct BuildArgs {}

#[derive(Debug, Args, Default)]
/// Arguments for local-play mod installation.
pub struct InstallArgs {
    #[arg(long, value_name = "PATH")]
    /// Overrides the default Project Zomboid mods root.
    pub root: Option<PathBuf>,
}

#[derive(Debug, Args, Default)]
/// Arguments for Workshop project construction.
pub struct PackageArgs {}

#[derive(Debug, Args, Default)]
/// Arguments for staging a Workshop project for Zomboid's uploader.
pub struct StageArgs {
    #[arg(long, value_name = "PATH")]
    /// Overrides the default Zomboid Workshop projects directory.
    pub root: Option<PathBuf>,
}

#[derive(Debug, Args, Default)]
/// Arguments for generated-artifact cleanup.
pub struct CleanArgs {}

#[derive(Debug, Args)]
/// Arguments for machine-specific user defaults.
pub struct ConfigArgs {
    /// Configuration operation to perform.
    #[command(subcommand)]
    pub command: ConfigCommand,
}

#[derive(Debug, Subcommand)]
/// Supported user-configuration operations.
pub enum ConfigCommand {
    /// Displays the configuration file and effective defaults.
    Show,
    /// Assigns a user default.
    Set {
        /// Setting to assign.
        key: ConfigKey,
        /// New author name or absolute directory path.
        value: String,
    },
    /// Removes a user default.
    Unset {
        /// Setting to remove.
        key: ConfigKey,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
/// Machine-specific setting names accepted by `km config`.
pub enum ConfigKey {
    /// Default author used by `km new`.
    Author,
    /// Default local Project Zomboid mods directory.
    ModsRoot,
    /// Default Project Zomboid Workshop projects directory.
    WorkshopRoot,
}

#[derive(Debug, Args)]
/// Arguments for shell completion generation.
pub struct CompletionsArgs {
    /// Shell whose completion script should be generated.
    pub shell: CompletionShell,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
/// Shells supported by completion generation.
pub enum CompletionShell {
    /// Bourne Again Shell.
    Bash,
    /// Elvish shell.
    Elvish,
    /// Friendly Interactive Shell.
    Fish,
    /// Microsoft PowerShell.
    Powershell,
    /// Z shell.
    Zsh,
}

impl From<CompletionShell> for clap_complete::Shell {
    fn from(shell: CompletionShell) -> Self {
        match shell {
            CompletionShell::Bash => Self::Bash,
            CompletionShell::Elvish => Self::Elvish,
            CompletionShell::Fish => Self::Fish,
            CompletionShell::Powershell => Self::PowerShell,
            CompletionShell::Zsh => Self::Zsh,
        }
    }
}

#[derive(Debug, Args, Default)]
/// Arguments for read-only environment and project diagnostics.
pub struct DoctorArgs {}

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
    #[arg(long, global = true, value_name = "PATH")]
    pub project: Option<PathBuf>,

    #[arg(short, long, global = true, conflicts_with = "verbose")]
    pub quiet: bool,

    #[arg(short, long, global = true, conflicts_with = "quiet")]
    pub verbose: bool,

    #[arg(long, global = true, value_enum, default_value_t = ColorChoice::Auto)]
    pub color: ColorChoice,

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
    New(NewArgs),
    Init(InitArgs),
    Doctor(DoctorArgs),
    Check(CheckArgs),
    Test(TestArgs),
    Build(BuildArgs),
    Install(InstallArgs),
    Package(PackageArgs),
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
    #[arg(long, default_value = "42")]
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

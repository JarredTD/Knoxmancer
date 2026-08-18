//! Application workflows connecting CLI input, core operations, and output.

use std::path::PathBuf;

use crate::build::{self, DevelopmentArtifact};
use crate::cli::Reporter;
use clap::CommandFactory;

use crate::cli::args::OutputFormat;
use crate::cli::{
    Cli, Command, CompletionShell, ConfigArgs, ConfigCommand, ConfigKey, NewArgs, OpenTarget,
};
use crate::error::{Error, Result};
use crate::project::validation::ValidationTarget;
use crate::project::{Project, ValidatedProject, config, validation};
use crate::scaffold::{self, NewProjectOptions};
use crate::system::user_config::{self, UserConfig};

/// Executes the selected command and reports its successful result.
pub(crate) fn run(cli: Cli, reporter: &Reporter) -> Result<()> {
    let project_start = cli.project;
    let quiet = cli.quiet;
    let format = cli.format;
    match cli.command {
        Command::New(args) => {
            let config = user_config::load()?.values;
            new_project(args, &config, reporter)
        }
        Command::Init(args) => {
            let root = scaffold::init_project(project_start.as_deref(), args.force)?;
            reporter.status(&format!("Initialized {}", root.display()));
            Ok(())
        }
        Command::Paths(_) => {
            let project = discover(project_start.as_deref())?;
            let config = user_config::load()?.values;
            paths(&project, &config, reporter)?;
            Ok(())
        }
        Command::Check(args) => {
            let project = discover(project_start.as_deref())?;
            let target = if args.workshop {
                ValidationTarget::Workshop
            } else {
                ValidationTarget::Playable
            };
            let validated = validation::check(&project, target)?;
            report_checked(&validated, reporter);
            Ok(())
        }
        Command::Build(_) => {
            let project = discover(project_start.as_deref())?;
            build(&project, reporter).map(|_| ())
        }
        Command::Install(args) => {
            let project = discover(project_start.as_deref())?;
            let built = build(&project, reporter)?;
            let config = user_config::load()?.values;
            let root = args.root.as_deref().or(config.mods_root.as_deref());
            let installed = build::install(&built, root)?;
            report_warnings(&installed.warnings, reporter);
            reporter.status(&format!(
                "Installed for local play: {}",
                installed.path.display()
            ));
            reporter.status("Next: enable the mod in Project Zomboid.");
            Ok(())
        }
        Command::Package(_) => {
            let project = discover(project_start.as_deref())?;
            let validated = validation::check(&project, ValidationTarget::Workshop)?;
            report_checked(&validated, reporter);
            let packaged = crate::workshop::package(&validated)?;
            report_warnings(&packaged.warnings, reporter);
            reporter.status(&format!(
                "Packaged Workshop project: {}",
                packaged.path.display()
            ));
            Ok(())
        }
        Command::Stage(args) => {
            let project = discover(project_start.as_deref())?;
            let validated = validation::check(&project, ValidationTarget::Workshop)?;
            report_checked(&validated, reporter);
            let packaged = crate::workshop::package(&validated)?;
            report_warnings(&packaged.warnings, reporter);
            let config = user_config::load()?.values;
            let root = args.root.as_deref().or(config.workshop_root.as_deref());
            let staged = crate::workshop::stage(&packaged, root)?;
            report_warnings(&staged.warnings, reporter);
            reporter.status(&format!(
                "Staged for Workshop upload: {}",
                staged.path.display()
            ));
            reporter.status("Next: open Workshop > Create and update items in Project Zomboid.");
            Ok(())
        }
        Command::Clean(_) => {
            let project = discover(project_start.as_deref())?;
            let result = build::clean(&project)?;
            if result.removed {
                reporter.status(&format!("Removed {}", result.path.display()));
            } else {
                reporter.status(&format!("Nothing to clean: {}", result.path.display()));
            }
            Ok(())
        }
        Command::Config(args) => configure(args, reporter),
        Command::Completions(args) => completions(args.shell, quiet, format),
        Command::Doctor(_) => doctor(project_start.as_deref(), reporter),
        Command::Open(args) => open(project_start.as_deref(), args.target, reporter),
    }
}

/// Opens one resolved directory in the platform file browser.
fn open(start: Option<&std::path::Path>, target: OpenTarget, reporter: &Reporter) -> Result<()> {
    let loaded = user_config::load()?;
    let project = discover(start)?;
    let validated = validation::check(&project, ValidationTarget::Playable)?;
    let resolved = resolved_paths(&validated, &loaded.values)?;
    let (label, path) = match target {
        OpenTarget::Artifact => ("development artifact", resolved.development),
        OpenTarget::Mods => ("local installation", resolved.local),
        OpenTarget::Package => ("Workshop package", resolved.workshop_artifact),
        OpenTarget::Workshop => ("Workshop staging", resolved.workshop_staging),
    };
    crate::system::opener::open(&path)?;
    reporter.status(&format!("Opened {label}: {}", path.display()));
    Ok(())
}

/// Runs full read-only project and environment readiness checks.
fn doctor(start: Option<&std::path::Path>, reporter: &Reporter) -> Result<()> {
    let loaded = user_config::load()?;
    let project = discover(start)?;
    let validated = validation::check(&project, ValidationTarget::Workshop)?;
    let resolved = resolved_paths(&validated, &loaded.values)?;
    reporter.status(if loaded.exists {
        "User configuration: loaded"
    } else {
        "User configuration: defaults"
    });
    reporter.path("project", "Project", &project.root);
    let mods_root = resolved
        .local
        .parent()
        .ok_or_else(|| Error::project("resolved local installation has no parent directory"))?;
    let workshop_root = resolved
        .workshop_staging
        .parent()
        .ok_or_else(|| Error::project("resolved Workshop staging has no parent directory"))?;
    reporter.path("mods_root", "Mods root", mods_root);
    reporter.path("workshop_root", "Workshop root", workshop_root);
    report_checked(&validated, reporter);
    reporter.status("Doctor: ready for local play and Workshop packaging.");
    Ok(())
}

/// Emits a raw completion script for the requested shell.
fn completions(shell: CompletionShell, quiet: bool, format: OutputFormat) -> Result<()> {
    if quiet {
        return Ok(());
    }
    if format == OutputFormat::Json {
        return Err(crate::error::Error::project(
            "--format json is not supported by the completions command",
        ));
    }
    clap_complete::generate(
        clap_complete::Shell::from(shell),
        &mut Cli::command(),
        "km",
        &mut std::io::stdout(),
    );
    Ok(())
}

/// Displays, assigns, or clears machine-specific defaults.
fn configure(args: ConfigArgs, reporter: &Reporter) -> Result<()> {
    let mut loaded = user_config::load()?;
    match args.command {
        ConfigCommand::Show => {
            reporter.path("user_configuration", "User configuration", &loaded.path);
            reporter.status(&format!(
                "Default author: {}",
                loaded.values.author.as_deref().unwrap_or("(not set)")
            ));
            let mods = crate::system::environment::zomboid_root(
                loaded.values.mods_root.as_deref(),
                "mods",
            )?;
            let workshop = crate::system::environment::zomboid_root(
                loaded.values.workshop_root.as_deref(),
                "Workshop",
            )?;
            reporter.path("mods_root", "Mods root", &mods);
            reporter.path("workshop_root", "Workshop root", &workshop);
            Ok(())
        }
        ConfigCommand::Set { key, value } => {
            match key {
                ConfigKey::Author => loaded.values.author = Some(value.trim().to_owned()),
                ConfigKey::ModsRoot => loaded.values.mods_root = Some(value.into()),
                ConfigKey::WorkshopRoot => loaded.values.workshop_root = Some(value.into()),
            }
            save_user_config(&loaded.path, &loaded.values, reporter)
        }
        ConfigCommand::Unset { key } => {
            match key {
                ConfigKey::Author => loaded.values.author = None,
                ConfigKey::ModsRoot => loaded.values.mods_root = None,
                ConfigKey::WorkshopRoot => loaded.values.workshop_root = None,
            }
            save_user_config(&loaded.path, &loaded.values, reporter)
        }
    }
}

/// Saves user defaults and presents replacement cleanup warnings.
fn save_user_config(
    path: &std::path::Path,
    config: &UserConfig,
    reporter: &Reporter,
) -> Result<()> {
    if let Some(warning) = user_config::save(path, config)? {
        reporter.warning(&warning);
    }
    reporter.status(&format!("Updated user configuration: {}", path.display()));
    Ok(())
}

/// Discovers a project from the optional command-line starting path.
fn discover(start: Option<&std::path::Path>) -> Result<Project> {
    config::Project::discover(start)
}

/// Creates a scaffold from parsed command-line arguments.
fn new_project(args: NewArgs, config: &UserConfig, reporter: &Reporter) -> Result<()> {
    let result = scaffold::new_project(&NewProjectOptions {
        directory: args.directory,
        name: args.name,
        id: args.id,
        author: args.author.or_else(|| config.author.clone()),
    })?;
    reporter.status(&format!(
        "Created {} (Build 42) at {}",
        result.name,
        result.root.display()
    ));
    reporter.status(&format!("Next: cd {} && km install", result.root.display()));
    Ok(())
}

/// Reports every generated and game-facing directory for a project.
fn paths(project: &Project, config: &UserConfig, reporter: &Reporter) -> Result<()> {
    let validated = validation::check(project, ValidationTarget::Playable)?;
    let resolved = resolved_paths(&validated, config)?;
    for (name, label, path) in [
        (
            "development_artifact",
            "Development artifact",
            resolved.development,
        ),
        ("local_installation", "Local installation", resolved.local),
        (
            "workshop_artifact",
            "Workshop artifact",
            resolved.workshop_artifact,
        ),
        (
            "workshop_staging",
            "Workshop staging",
            resolved.workshop_staging,
        ),
    ] {
        reporter.path(name, label, &path);
    }
    Ok(())
}

/// Fully resolved artifact and game-facing paths for one validated project.
struct ResolvedPaths {
    /// Generated local development artifact.
    development: PathBuf,
    /// Installed local mod directory.
    local: PathBuf,
    /// Generated Workshop upload package.
    workshop_artifact: PathBuf,
    /// Project Zomboid Workshop staging directory.
    workshop_staging: PathBuf,
}

/// Resolves paths using explicit user defaults before platform conventions.
fn resolved_paths(validated: &ValidatedProject<'_>, config: &UserConfig) -> Result<ResolvedPaths> {
    let output = validated.layout.output_root()?;
    let mod_id = &validated.metadata.id;
    let mods = crate::system::environment::zomboid_root(config.mods_root.as_deref(), "mods")?;
    let workshop =
        crate::system::environment::zomboid_root(config.workshop_root.as_deref(), "Workshop")?;
    Ok(ResolvedPaths {
        development: output.join("dev").join(mod_id),
        local: mods.join(mod_id),
        workshop_artifact: output.join("workshop").join(mod_id),
        workshop_staging: workshop.join(mod_id),
    })
}

/// Validates and builds a local development artifact.
fn build(project: &Project, reporter: &Reporter) -> Result<DevelopmentArtifact> {
    let validated = validation::check(project, ValidationTarget::Playable)?;
    report_checked(&validated, reporter);
    let artifact = build::build(&validated)?;
    report_warnings(&artifact.warnings, reporter);
    reporter.status(&format!("Built artifact: {}", artifact.path.display()));
    Ok(artifact)
}

/// Emits non-fatal filesystem cleanup warnings.
fn report_warnings(warnings: &[String], reporter: &Reporter) {
    for warning in warnings {
        reporter.warning(warning);
    }
}

/// Reports the validated mod identity, version, and supported build.
fn report_checked(validated: &ValidatedProject<'_>, reporter: &Reporter) {
    reporter.status(&format!(
        "Checked {} {} (Build {})",
        validated.metadata.name, validated.metadata.version, validated.project.config.project.build
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn reports_each_cleanup_warning() {
        let cli = Cli::try_parse_from(["km", "--quiet", "--format", "json", "check"]).unwrap();
        let reporter = Reporter::new(cli.output_options());
        report_warnings(&["first".to_owned(), "second".to_owned()], &reporter);
    }
}

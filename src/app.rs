//! Application workflows connecting CLI input, core operations, and output.

use crate::build::{self, DevelopmentArtifact};
use crate::cli::Reporter;
use crate::cli::{Cli, Command, NewArgs};
use crate::error::Result;
use crate::project::validation::ValidationTarget;
use crate::project::{Project, ValidatedProject, config, validation};
use crate::scaffold::{self, NewProjectOptions};

/// Executes the selected command and reports its successful result.
pub(crate) fn run(cli: Cli, reporter: &Reporter) -> Result<()> {
    let project_start = cli.project;
    match cli.command {
        Command::New(args) => new_project(args, reporter),
        Command::Init(args) => {
            let root = scaffold::init_project(project_start.as_deref(), args.force)?;
            reporter.status(&format!("Initialized {}", root.display()));
            Ok(())
        }
        Command::Paths(_) => {
            let project = discover(project_start.as_deref())?;
            paths(&project, reporter)?;
            Ok(())
        }
        Command::Check(_) => {
            let project = discover(project_start.as_deref())?;
            let validated = validation::check(&project, ValidationTarget::Playable)?;
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
            let installed = build::install(&built, args.root.as_deref())?;
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
            let staged = crate::workshop::stage(&packaged, args.root.as_deref())?;
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
    }
}

/// Discovers a project from the optional command-line starting path.
fn discover(start: Option<&std::path::Path>) -> Result<Project> {
    config::Project::discover(start)
}

/// Creates a scaffold from parsed command-line arguments.
fn new_project(args: NewArgs, reporter: &Reporter) -> Result<()> {
    let result = scaffold::new_project(&NewProjectOptions {
        directory: args.directory,
        name: args.name,
        id: args.id,
        author: args.author,
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
fn paths(project: &Project, reporter: &Reporter) -> Result<()> {
    let validated = validation::check(project, ValidationTarget::Playable)?;
    let output = validated.layout.output_root()?;
    let mod_id = &validated.metadata.id;
    let local = crate::system::environment::zomboid_root(None, "mods")?.join(mod_id);
    let staging = crate::system::environment::zomboid_root(None, "Workshop")?.join(mod_id);
    for (label, path) in [
        ("Development artifact", output.join("dev").join(mod_id)),
        ("Local installation", local),
        ("Workshop artifact", output.join("workshop").join(mod_id)),
        ("Workshop staging", staging),
    ] {
        reporter.status(&format!("{label}: {}", path.display()));
    }
    Ok(())
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

/// Reports the validated mod identity, version, and supported builds.
fn report_checked(validated: &ValidatedProject<'_>, reporter: &Reporter) {
    reporter.status(&format!(
        "Checked {} {} ({})",
        validated.metadata.name,
        validated.metadata.version,
        validated
            .project
            .config
            .project
            .builds
            .iter()
            .map(|build| format!("Build {build}"))
            .collect::<Vec<_>>()
            .join(", ")
    ));
}

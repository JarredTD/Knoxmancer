//! Application workflows connecting CLI input, core operations, and output.

use crate::artifact::{self, BuildArtifact, BuildProfile};
use crate::cli::{Cli, Command, NewArgs};
use crate::config::{self, Project};
use crate::error::Result;
use crate::output::Reporter;
use crate::scaffold::{self, NewProjectOptions};
use crate::validation::{self, ValidatedProject};

pub(crate) fn run(cli: Cli, reporter: &Reporter) -> Result<()> {
    let project_start = cli.project;
    match cli.command {
        Command::New(args) => new_project(args, reporter),
        Command::Init(args) => {
            let root = scaffold::init_project(project_start.as_deref(), args.force)?;
            reporter.status(&format!("Initialized {}", root.display()));
            Ok(())
        }
        Command::Doctor(_) => {
            for line in crate::environment::doctor() {
                reporter.status(&line);
            }
            Ok(())
        }
        Command::Check(args) => {
            let project = discover(project_start.as_deref())?;
            let validated = validation::check(&project, args.release)?;
            report_checked(&validated, reporter);
            Ok(())
        }
        Command::Test(_) => {
            let project = discover(project_start.as_deref())?;
            let validated = validation::check(&project, false)?;
            report_checked(&validated, reporter);
            reporter.status(&format!(
                "Running {}",
                project.config.test.command.join(" ")
            ));
            crate::test_runner::run(&validated)?;
            reporter.status("Tests passed");
            Ok(())
        }
        Command::Build(args) => {
            let project = discover(project_start.as_deref())?;
            build(&project, profile(args.release), reporter).map(|_| ())
        }
        Command::Install(args) => {
            let project = discover(project_start.as_deref())?;
            let built = build(&project, profile(args.release), reporter)?;
            let destination = artifact::install(&built, args.root.as_deref())?;
            reporter.status(&format!("Installed {}", destination.display()));
            Ok(())
        }
        Command::Package(_) => {
            let project = discover(project_start.as_deref())?;
            let validated = validation::check(&project, true)?;
            report_checked(&validated, reporter);
            let built = build_validated(&validated, BuildProfile::Release, reporter)?;
            let destination = crate::workshop::package(&validated, &built)?;
            reporter.status(&format!(
                "Packaged Workshop artifact: {}",
                destination.display()
            ));
            Ok(())
        }
        Command::Clean(_) => {
            let project = discover(project_start.as_deref())?;
            let result = artifact::clean(&project)?;
            if result.removed {
                reporter.status(&format!("Removed {}", result.path.display()));
            } else {
                reporter.status(&format!("Nothing to clean: {}", result.path.display()));
            }
            Ok(())
        }
    }
}

fn discover(start: Option<&std::path::Path>) -> Result<Project> {
    config::Project::discover(start)
}

fn new_project(args: NewArgs, reporter: &Reporter) -> Result<()> {
    let result = scaffold::new_project(&NewProjectOptions {
        directory: args.directory,
        name: args.name,
        id: args.id,
        author: args.author,
        build: args.build,
    })?;
    reporter.status(&format!(
        "Created {} (Build {}) at {}",
        result.name,
        result.build,
        result.root.display()
    ));
    reporter.status(&format!("Next: cd {} && km install", result.root.display()));
    Ok(())
}

fn build(project: &Project, profile: BuildProfile, reporter: &Reporter) -> Result<BuildArtifact> {
    let validated = validation::check(project, profile == BuildProfile::Release)?;
    report_checked(&validated, reporter);
    build_validated(&validated, profile, reporter)
}

fn build_validated(
    validated: &ValidatedProject<'_>,
    profile: BuildProfile,
    reporter: &Reporter,
) -> Result<BuildArtifact> {
    reporter.verbose("Staging artifact with atomic replacement");
    let artifact = artifact::build(validated, profile)?;
    if profile == BuildProfile::Release && validated.project.config.release.minify.is_some() {
        reporter.status(&format!("Minified {} Lua files", artifact.minified_files));
    }
    reporter.status(&format!(
        "Built {} artifact: {}",
        artifact.profile.name(),
        artifact.path.display()
    ));
    Ok(artifact)
}

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

fn profile(release: bool) -> BuildProfile {
    if release {
        BuildProfile::Release
    } else {
        BuildProfile::Development
    }
}

//! Application workflows connecting CLI input, core operations, and output.

use std::io::Write;
use std::path::PathBuf;

use crate::build::{self, DevelopmentArtifact};
use crate::cli::Reporter;
use clap::CommandFactory;

use crate::cli::args::OutputFormat;
use crate::cli::{
    Cli, Command, CompletionsArgs, ConfigArgs, ConfigCommand, ConfigKey, NewArgs, OpenTarget,
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
        Command::Copies(_) => {
            let project = discover(project_start.as_deref())?;
            let config = user_config::load()?.values;
            copies(&project, &config, reporter)
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
            if args.live {
                reporter.warning(
                    "live installation is non-atomic; exit the world and remain at the main menu until synchronization finishes",
                );
                let installed = build::install_live(&built, root)?;
                for operation in &installed.operations {
                    reporter.live_operation(operation);
                }
                report_live_summary(&installed, reporter);
                if installed.has_non_lua_changes() {
                    reporter.warning(
                        "non-Lua files changed; restarting Project Zomboid may be required",
                    );
                }
                if installed.has_lua_topology_changes() {
                    reporter.warning(
                        "Lua files were added or removed; Reload Lua may not discover the new file set, so a restart may be required",
                    );
                }
                if !installed.is_complete() {
                    return Err(Error::io(std::io::Error::other(format!(
                        "live installation was incomplete at {}; review the failed and skipped operations above",
                        installed.path.display()
                    ))));
                }
                reporter.status(&format!(
                    "Live-installed for local play: {}",
                    installed.path.display()
                ));
                reporter.status("Next: at the main menu, use Reload Lua before loading the world.");
            } else {
                let installed = build::install(&built, root)?;
                report_warnings(&installed.warnings, reporter);
                reporter.status(&format!(
                    "Installed for local play: {}",
                    installed.path.display()
                ));
                reporter.status("Next: enable the mod in Project Zomboid.");
            }
            let copies = discover_copies_for(&built.mod_id, &built.build, &config, root, None)?;
            report_copies(&copies, &built.version, reporter);
            report_stale_staging(&copies, &built.version, reporter);
            if crate::system::copies::has_playable_conflict(&copies, &built.version) {
                reporter.warning(
                    "playable mod copies conflict; run `km copies` and unsubscribe the Steam copy while testing locally",
                );
            }
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
            let copies = discover_copies_for(
                &validated.metadata.id,
                &validated.metadata.build,
                &config,
                None,
                root,
            )?;
            let expected = staged
                .path
                .join("Contents/mods")
                .join(&validated.metadata.id);
            if !copies.iter().any(|copy| {
                copy.source == crate::system::copies::CopySource::Staging
                    && copy.path == expected
                    && copy.is_current(&validated.metadata.version)
            }) {
                return Err(Error::project(
                    "Workshop staging verification failed after replacement",
                ));
            }
            reporter.status(&format!(
                "Verified Workshop staging: {}",
                validated.metadata.version
            ));
            report_stale_staging(&copies, &validated.metadata.version, reporter);
            if crate::system::copies::has_playable_conflict(&copies, &validated.metadata.version) {
                reporter.warning(
                    "playable mod copies still conflict; run `km copies` before testing locally",
                );
            }
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
        Command::Completions(args) => completions(args, quiet, format, reporter),
        Command::Doctor(_) => doctor(project_start.as_deref(), reporter),
        Command::Open(args) => open(project_start.as_deref(), args.target, reporter),
    }
}

/// Reports aggregate live-install file operation counts.
fn report_live_summary(installed: &build::LiveInstallResult, reporter: &Reporter) {
    let count = |action, status| installed.count(action, status);
    let failed = installed
        .operations
        .iter()
        .filter(|operation| operation.status == build::LiveStatus::Failed)
        .count();
    let skipped = installed
        .operations
        .iter()
        .filter(|operation| operation.status == build::LiveStatus::Skipped)
        .count();
    reporter.status(&format!(
        "Live install: {} created, {} updated, {} removed, {} unchanged, {failed} failed, {skipped} skipped",
        count(build::LiveAction::Create, build::LiveStatus::Applied),
        count(build::LiveAction::Update, build::LiveStatus::Applied),
        count(build::LiveAction::Remove, build::LiveStatus::Applied),
        count(build::LiveAction::Unchanged, build::LiveStatus::Unchanged),
    ));
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
    let Some(mods_root) = resolved.local.parent() else {
        return Err(Error::project(
            "resolved local installation has no parent directory",
        ));
    };
    let Some(workshop_root) = resolved.workshop_staging.parent() else {
        return Err(Error::project(
            "resolved Workshop staging has no parent directory",
        ));
    };
    reporter.path("mods_root", "Mods root", mods_root);
    reporter.path("workshop_root", "Workshop root", workshop_root);
    report_checked(&validated, reporter);
    let copies = discover_copies(&validated, &loaded.values)?;
    report_copies(&copies, &validated.metadata.version, reporter);
    report_stale_staging(&copies, &validated.metadata.version, reporter);
    if crate::system::copies::has_playable_conflict(&copies, &validated.metadata.version) {
        return Err(Error::project(
            "installed mod copies conflict; run `km copies` and unsubscribe stale Steam copies",
        ));
    }
    reporter.status("Doctor: ready for local play and Workshop packaging.");
    Ok(())
}

/// Emits a raw completion script for the requested shell.
fn completions(
    args: CompletionsArgs,
    quiet: bool,
    format: OutputFormat,
    reporter: &Reporter,
) -> Result<()> {
    if quiet && args.output.is_none() {
        return Ok(());
    }
    if format == OutputFormat::Json {
        return Err(crate::error::Error::project(
            "--format json is not supported by the completions command",
        ));
    }
    let mut script = Vec::new();
    clap_complete::generate(
        clap_complete::Shell::from(args.shell),
        &mut Cli::command(),
        args.bin.as_str(),
        &mut script,
    );
    if let Some(output) = args.output {
        let output = if output.is_absolute() {
            output
        } else {
            std::env::current_dir().map_err(Error::io)?.join(output)
        };
        let replacement = crate::system::fs::atomic_write(&output, &script)?;
        if let Some(warning) = replacement.cleanup_warning {
            reporter.warning(&warning);
        }
        reporter.path("completion_script", "Completion script", &output);
    } else {
        let _ = std::io::stdout().write_all(&script);
    }
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
            if let Some(steam) = &loaded.values.steam_root {
                reporter.path("steam_root", "Steam root", steam);
            }
            Ok(())
        }
        ConfigCommand::Set { key, value } => {
            match key {
                ConfigKey::Author => loaded.values.author = Some(value.trim().to_owned()),
                ConfigKey::ModsRoot => loaded.values.mods_root = Some(value.into()),
                ConfigKey::WorkshopRoot => loaded.values.workshop_root = Some(value.into()),
                ConfigKey::SteamRoot => loaded.values.steam_root = Some(value.into()),
            }
            save_user_config(&loaded.path, &loaded.values, reporter)
        }
        ConfigCommand::Unset { key } => {
            match key {
                ConfigKey::Author => loaded.values.author = None,
                ConfigKey::ModsRoot => loaded.values.mods_root = None,
                ConfigKey::WorkshopRoot => loaded.values.workshop_root = None,
                ConfigKey::SteamRoot => loaded.values.steam_root = None,
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

/// Reports copies of the current mod found in game-facing directories.
fn copies(project: &Project, config: &UserConfig, reporter: &Reporter) -> Result<()> {
    let validated = validation::check(project, ValidationTarget::Playable)?;
    let copies = discover_copies(&validated, config)?;
    if copies.is_empty() {
        reporter.status("No installed copies found.");
        return Ok(());
    }
    report_copies(&copies, &validated.metadata.version, reporter);
    report_stale_staging(&copies, &validated.metadata.version, reporter);
    if crate::system::copies::has_playable_conflict(&copies, &validated.metadata.version) {
        reporter.warning(
            "multiple or outdated playable copies can make Project Zomboid load unexpected files",
        );
    }
    Ok(())
}

/// Emits each installed-copy record using the active output format.
fn report_copies(
    copies: &[crate::system::copies::InstalledCopy],
    version: &str,
    reporter: &Reporter,
) {
    for copy in copies {
        reporter.mod_copy(
            copy.source.as_str(),
            copy.source.label(),
            copy.version.as_deref(),
            copy.is_current(version),
            &copy.path,
        );
    }
}

/// Recommends refreshing uploader staging when it does not match the project.
fn report_stale_staging(
    copies: &[crate::system::copies::InstalledCopy],
    version: &str,
    reporter: &Reporter,
) {
    if copies.iter().any(|copy| {
        copy.source == crate::system::copies::CopySource::Staging && !copy.is_current(version)
    }) {
        reporter.warning("Workshop staging is outdated; run `km stage` to refresh it");
    }
}

/// Discovers copies using the same configured game-facing roots as other commands.
fn discover_copies(
    validated: &ValidatedProject<'_>,
    config: &UserConfig,
) -> Result<Vec<crate::system::copies::InstalledCopy>> {
    discover_copies_for(
        &validated.metadata.id,
        &validated.metadata.build,
        config,
        None,
        None,
    )
}

/// Discovers copies for explicit project identity and optional install root.
fn discover_copies_for(
    mod_id: &str,
    build: &str,
    config: &UserConfig,
    mods_override: Option<&std::path::Path>,
    workshop_override: Option<&std::path::Path>,
) -> Result<Vec<crate::system::copies::InstalledCopy>> {
    let mods = crate::system::environment::zomboid_root(
        mods_override.or(config.mods_root.as_deref()),
        "mods",
    )?;
    let workshop = crate::system::environment::zomboid_root(
        workshop_override.or(config.workshop_root.as_deref()),
        "Workshop",
    )?;
    crate::system::copies::discover(
        mod_id,
        build,
        &mods,
        &workshop,
        config.steam_root.as_deref(),
    )
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

    #[test]
    fn reports_a_complete_live_install_summary() {
        let cli = Cli::try_parse_from(["km", "--quiet", "install"]).unwrap();
        let reporter = Reporter::new(cli.output_options());
        let operations = [
            (build::LiveAction::Create, build::LiveStatus::Applied),
            (build::LiveAction::Update, build::LiveStatus::Applied),
            (build::LiveAction::Remove, build::LiveStatus::Applied),
            (build::LiveAction::Unchanged, build::LiveStatus::Unchanged),
            (build::LiveAction::Verify, build::LiveStatus::Failed),
            (build::LiveAction::Remove, build::LiveStatus::Skipped),
        ]
        .into_iter()
        .map(|(action, status)| build::LiveOperation {
            action,
            status,
            path: "42/file.lua".into(),
            message: None,
        })
        .collect();
        let installed = build::LiveInstallResult {
            path: "Example".into(),
            operations,
        };

        report_live_summary(&installed, &reporter);
    }
}

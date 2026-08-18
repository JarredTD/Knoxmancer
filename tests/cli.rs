use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::tempdir;

fn km(arguments: &[&str]) -> Output {
    let config = Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test-user-config.toml");
    km_with_config(arguments, &config)
}

fn km_with_config(arguments: &[&str], config: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_km"))
        .args(arguments)
        .env("KNOXMANCER_CONFIG", config)
        .output()
        .expect("km command should run")
}

fn path(path: &Path) -> &str {
    path.to_str().expect("temporary path should be UTF-8")
}

fn files_below(root: &Path) -> BTreeSet<String> {
    fn visit(root: &Path, directory: &Path, files: &mut BTreeSet<String>) {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(root, &path, files);
            } else {
                files.insert(
                    path.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }

    let mut files = BTreeSet::new();
    visit(root, root, &mut files);
    files
}

fn copy_fixture(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let target = destination.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_fixture(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

#[test]
fn scaffolds_checks_builds_packages_installs_and_cleans() {
    let temporary = tempdir().unwrap();
    let project = temporary.path().join("example-mod");
    let mods = temporary.path().join("mods");
    let workshop_projects = temporary.path().join("workshop-projects");

    assert!(
        km(&["new", path(&project), "--author", "Test Author"])
            .status
            .success()
    );
    assert!(km(&["--project", path(&project), "check"]).status.success());
    assert!(km(&["--project", path(&project), "build"]).status.success());
    assert!(
        km(&[
            "--project",
            path(&project),
            "stage",
            "--root",
            path(&workshop_projects),
        ])
        .status
        .success()
    );
    assert!(
        km(&[
            "--project",
            path(&project),
            "install",
            "--root",
            path(&mods)
        ])
        .status
        .success()
    );

    let playable_files = BTreeSet::from(["42/mod.info".to_owned()]);
    assert_eq!(
        files_below(&project.join("dist/dev/ExampleMod")),
        playable_files
    );
    assert!(!project.join("dist/release").exists());
    let mut workshop_files = playable_files
        .iter()
        .map(|file| format!("Contents/mods/ExampleMod/{file}"))
        .collect::<BTreeSet<_>>();
    workshop_files.extend(["preview.png".to_owned(), "workshop.txt".to_owned()]);
    assert_eq!(
        files_below(&project.join("dist/workshop/ExampleMod")),
        workshop_files
    );
    assert_eq!(files_below(&mods.join("ExampleMod")), playable_files);
    assert_eq!(
        files_below(&workshop_projects.join("ExampleMod")),
        workshop_files
    );

    assert!(km(&["--project", path(&project), "clean"]).status.success());
    assert!(!project.join("dist").exists());
}

#[test]
fn initializes_an_existing_mod_without_rewriting_metadata() {
    let temporary = tempdir().unwrap();
    let metadata = temporary.path().join("src/mod.info");
    fs::create_dir_all(metadata.parent().unwrap()).unwrap();
    fs::write(&metadata, "name=Existing\nid=Existing\nmodversion=1.0.0\n").unwrap();

    assert!(
        km(&["--project", path(temporary.path()), "init"])
            .status
            .success()
    );
    assert!(temporary.path().join("knoxmancer.toml").is_file());
    assert_eq!(
        fs::read_to_string(metadata).unwrap(),
        "name=Existing\nid=Existing\nmodversion=1.0.0\n"
    );
}

#[test]
fn reports_human_readable_errors() {
    let temporary = tempdir().unwrap();
    let output = km(&["--project", path(temporary.path()), "check"]);
    assert!(!output.status.success());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("no knoxmancer.toml found")
    );

    let project = temporary.path().join("diagnostic-mod");
    assert!(
        km(&["new", path(&project), "--author", "Tester"])
            .status
            .success()
    );
    fs::write(project.join("src/mod.info"), "name=Broken\n").unwrap();
    let output = km(&["--project", path(&project), "check"]);
    assert!(!output.status.success());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("mod.info")
    );
}

#[test]
fn full_and_short_binaries_expose_the_same_version() {
    let short = Command::new(env!("CARGO_BIN_EXE_km"))
        .arg("--version")
        .output()
        .unwrap();
    let full = Command::new(env!("CARGO_BIN_EXE_knoxmancer"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(short.status.success());
    assert!(full.status.success());
    assert_eq!(short.stdout, full.stdout);

    let help = km(&["--help"]);
    let help = String::from_utf8(help.stdout).unwrap();
    assert!(help.contains("Creates a complete Project Zomboid mod project"));
    assert!(help.contains("Creates a verified Workshop upload project"));
    assert!(help.contains("Packages and stages the mod for Zomboid's Workshop uploader"));
    let package_help = km(&["package", "--help"]);
    let package_help = String::from_utf8(package_help.stdout).unwrap();
    assert!(!package_help.contains("--stage"));
    assert!(!package_help.contains("--root"));
    let stage_help = String::from_utf8(km(&["stage", "--help"]).stdout).unwrap();
    assert!(stage_help.contains("--root"));
    let build_help = String::from_utf8(km(&["build", "--help"]).stdout).unwrap();
    let install_help = String::from_utf8(km(&["install", "--help"]).stdout).unwrap();
    assert!(!build_help.contains("--release"));
    assert!(!install_help.contains("--release"));
    for command in ["config", "completions", "doctor", "open"] {
        assert!(help.contains(command), "missing {command} command");
    }
}

#[test]
fn generates_supported_shell_completions() {
    for shell in ["bash", "elvish", "fish", "powershell", "zsh"] {
        let output = km(&["completions", shell]);
        assert!(
            output.status.success(),
            "completion generation failed for {shell}"
        );
        assert!(!output.stdout.is_empty());
        assert!(output.stderr.is_empty());
    }

    let quiet = km(&["--quiet", "completions", "bash"]);
    assert!(quiet.status.success());
    assert!(quiet.stdout.is_empty());
    assert!(
        !km(&["--format", "json", "completions", "bash"])
            .status
            .success()
    );
}

#[test]
fn doctor_reports_readiness_without_writing_artifacts() {
    let temporary = tempdir().unwrap();
    let config = temporary.path().join("config.toml");
    let project = temporary.path().join("doctor-mod");
    assert!(
        km_with_config(&["new", path(&project)], &config)
            .status
            .success()
    );

    let defaults = km_with_config(&["--project", path(&project), "doctor"], &config);
    assert!(defaults.status.success());
    assert!(
        String::from_utf8(defaults.stdout)
            .unwrap()
            .contains("User configuration: defaults")
    );

    assert!(
        km_with_config(&["config", "set", "author", "Doctor"], &config)
            .status
            .success()
    );

    let doctor = km_with_config(&["--project", path(&project), "doctor"], &config);
    assert!(doctor.status.success());
    let doctor = String::from_utf8(doctor.stdout).unwrap();
    assert!(doctor.contains("Doctor: ready"));
    assert!(doctor.contains("Mods root:"));
    assert!(doctor.contains("Workshop root:"));
    assert!(!project.join("dist").exists());

    fs::write(project.join("public/preview.png"), "invalid").unwrap();
    assert!(
        !km_with_config(&["--project", path(&project), "doctor"], &config)
            .status
            .success()
    );
}

#[test]
fn open_rejects_targets_that_have_not_been_created() {
    let temporary = tempdir().unwrap();
    let config = temporary.path().join("config.toml");
    let project = temporary.path().join("open-mod");
    assert!(
        km_with_config(&["new", path(&project)], &config)
            .status
            .success()
    );

    for target in ["artifact", "mods", "package", "workshop"] {
        let output = km_with_config(&["--project", path(&project), "open", target], &config);
        assert!(!output.status.success());
        assert!(
            String::from_utf8(output.stderr)
                .unwrap()
                .contains("does not exist yet")
        );
    }
}

#[test]
fn reports_resolved_project_paths() {
    let temporary = tempdir().unwrap();
    let project = temporary.path().join("paths-mod");
    assert!(km(&["new", path(&project)]).status.success());
    let output = km(&["--project", path(&project), "paths"]);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let stdout = String::from_utf8(output.stdout).unwrap();
    for label in [
        "Development artifact:",
        "Local installation:",
        "Workshop artifact:",
        "Workshop staging:",
    ] {
        assert!(stdout.contains(label), "missing {label}");
    }
}

#[test]
fn manages_machine_specific_user_defaults() {
    let temporary = tempdir().unwrap();
    let config = temporary.path().join("settings/config.toml");
    let mods = temporary.path().join("mods");
    let workshop = temporary.path().join("workshop");
    let project = temporary.path().join("managed-mod");

    let relative_location = km_with_config(&["config", "show"], Path::new("config.toml"));
    assert_eq!(relative_location.status.code(), Some(1));
    assert!(
        String::from_utf8(relative_location.stderr)
            .unwrap()
            .contains("must contain an absolute path")
    );

    let invalid_config = temporary.path().join("invalid.toml");
    fs::write(&invalid_config, "unknown = true\n").unwrap();
    assert_eq!(
        km_with_config(&["config", "show"], &invalid_config)
            .status
            .code(),
        Some(1)
    );

    assert_eq!(
        km_with_config(&["config", "set", "mods-root", "relative"], &config)
            .status
            .code(),
        Some(1)
    );
    assert_eq!(
        km_with_config(&["config", "set", "author", "bad\nauthor"], &config)
            .status
            .code(),
        Some(1)
    );

    assert!(
        km_with_config(&["config", "show"], &config)
            .status
            .success()
    );
    assert!(
        km_with_config(&["config", "set", "author", "Test Author"], &config)
            .status
            .success()
    );
    assert!(
        km_with_config(&["config", "set", "mods-root", path(&mods)], &config)
            .status
            .success()
    );
    assert!(
        km_with_config(
            &["config", "set", "workshop-root", path(&workshop)],
            &config,
        )
        .status
        .success()
    );
    let shown = km_with_config(&["config", "show"], &config);
    let shown = String::from_utf8(shown.stdout).unwrap();
    assert!(shown.contains("Test Author"));
    assert!(shown.contains(&mods.display().to_string()));
    assert!(shown.contains(&workshop.display().to_string()));

    assert!(
        km_with_config(&["new", path(&project)], &config)
            .status
            .success()
    );
    assert!(
        fs::read_to_string(project.join("src/mod.info"))
            .unwrap()
            .contains("author=Test Author")
    );
    assert!(
        km_with_config(&["--project", path(&project), "install"], &config)
            .status
            .success()
    );
    assert!(mods.join("ManagedMod/42/mod.info").is_file());
    assert!(
        km_with_config(&["--project", path(&project), "stage"], &config)
            .status
            .success()
    );
    assert!(workshop.join("ManagedMod/workshop.txt").is_file());

    assert!(
        km_with_config(&["config", "unset", "author"], &config)
            .status
            .success()
    );
    assert!(
        km_with_config(&["config", "unset", "mods-root"], &config)
            .status
            .success()
    );
    assert!(
        km_with_config(&["config", "unset", "workshop-root"], &config)
            .status
            .success()
    );
    assert!(!fs::read_to_string(&config).unwrap().contains("author"));
    assert!(
        !km_with_config(&["config", "set", "mods-root", "relative"], &config)
            .status
            .success()
    );

    let directory_config = temporary.path().join("directory-config.toml");
    fs::create_dir(&directory_config).unwrap();
    assert_eq!(
        km_with_config(&["config", "set", "author", "Blocked"], &directory_config,)
            .status
            .code(),
        Some(1)
    );
}

#[test]
fn packages_the_build_42_compatibility_fixture_exactly() {
    let temporary = tempdir().unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/build42");
    let project = temporary.path().join("fixture");
    let preview_source = temporary.path().join("preview-source");
    copy_fixture(&fixture, &project);
    assert!(km(&["new", path(&preview_source)]).status.success());
    fs::copy(
        preview_source.join("public/preview.png"),
        project.join("public/preview.png"),
    )
    .unwrap();

    assert!(km(&["--project", path(&project), "check"]).status.success());
    assert!(
        km(&["--project", path(&project), "package"])
            .status
            .success()
    );

    let expected = fs::read_to_string(fixture.join("expected-workshop-files.txt"))
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect();
    let artifact = project.join("dist/workshop/Build42Fixture");
    assert_eq!(files_below(&artifact), expected);
    assert_eq!(
        fs::read_to_string(artifact.join("Contents/mods/Build42Fixture/42/mod.info")).unwrap(),
        fs::read_to_string(fixture.join("src/mod.info")).unwrap()
    );
    let workshop = fs::read_to_string(artifact.join("workshop.txt")).unwrap();
    assert!(workshop.contains("[h1]Build 42 Compatibility Fixture[/h1]"));
    assert!(workshop.contains("[url=https://projectzomboid.com]Project Zomboid[/url]"));
}

#[test]
fn emits_stable_json_lines_for_paths_status_and_errors() {
    let temporary = tempdir().unwrap();
    let project = temporary.path().join("json-mod");
    assert!(km(&["new", path(&project)]).status.success());

    let paths = km(&["--format", "json", "--project", path(&project), "paths"]);
    assert!(paths.status.success());
    assert!(paths.stderr.is_empty());
    let events = String::from_utf8(paths.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(events.len(), 4);
    assert!(events.iter().all(|event| event["type"] == "path"));
    assert_eq!(events[0]["name"], "development_artifact");

    let checked = km(&["--format", "json", "--project", path(&project), "check"]);
    assert!(checked.status.success());
    assert!(checked.stderr.is_empty());
    let event: serde_json::Value =
        serde_json::from_slice(checked.stdout.strip_suffix(b"\n").unwrap()).unwrap();
    assert_eq!(event["type"], "status");

    fs::write(project.join("src/mod.info"), "name=Broken\n").unwrap();
    let failed = km(&["--format", "json", "--project", path(&project), "check"]);
    assert_eq!(failed.status.code(), Some(1));
    assert!(failed.stdout.is_empty());
    let event: serde_json::Value =
        serde_json::from_slice(failed.stderr.strip_suffix(b"\n").unwrap()).unwrap();
    assert_eq!(event["type"], "error");
    assert_eq!(event["kind"], "validation");
    assert_eq!(event["exit_code"], 1);
}

#[test]
fn rejects_unknown_commands_with_a_usage_exit_code() {
    let output = km(&["unknown-command"]);
    assert_eq!(output.status.code(), Some(2));

    let output = km(&["--format=json", "unknown-command"]);
    assert_eq!(output.status.code(), Some(2));
    let event: serde_json::Value =
        serde_json::from_slice(output.stderr.strip_suffix(b"\n").unwrap()).unwrap();
    assert_eq!(event["type"], "error");
    assert_eq!(event["kind"], "usage");
    assert_eq!(event["exit_code"], 2);
}

#[test]
fn scopes_validation_to_the_requested_artifact() {
    let temporary = tempdir().unwrap();
    let project = temporary.path().join("validation-mod");
    assert!(
        km(&["new", path(&project), "--author", "Tester"])
            .status
            .success()
    );

    fs::remove_file(project.join("CHANGELOG.md")).unwrap();
    assert!(km(&["--project", path(&project), "check"]).status.success());
    assert!(km(&["--project", path(&project), "build"]).status.success());

    let preview = project.join("public/preview.png");
    let original_preview = fs::read(&preview).unwrap();
    fs::write(&preview, b"not a png").unwrap();
    assert!(km(&["--project", path(&project), "check"]).status.success());
    assert!(
        !km(&["--project", path(&project), "check", "--workshop"])
            .status
            .success()
    );
    assert!(
        !km(&["--project", path(&project), "package"])
            .status
            .success()
    );
    fs::write(&preview, original_preview).unwrap();

    let translation = project.join("src/shared/Translate/EN/UI.json");
    fs::create_dir_all(translation.parent().unwrap()).unwrap();
    fs::write(&translation, "[]").unwrap();
    assert!(km(&["--project", path(&project), "check"]).status.success());
    assert!(
        km(&["--project", path(&project), "package"])
            .status
            .success()
    );
}

#[test]
fn preserves_sources_and_replaces_existing_artifacts() {
    let temporary = tempdir().unwrap();
    let project = temporary.path().join("release-mod");
    let mods = temporary.path().join("mods");
    assert!(
        km(&["new", path(&project), "--author", "Tester"])
            .status
            .success()
    );
    fs::write(project.join("LICENSE"), "Example license\n").unwrap();
    let manifest = project.join("knoxmancer.toml");
    let configured = fs::read_to_string(&manifest)
        .unwrap()
        .replace("include = []", "include = [\"CHANGELOG.md\", \"LICENSE\"]");
    fs::write(&manifest, configured).unwrap();
    for directory in ["src/client", "src/shared", "src/media"] {
        fs::create_dir_all(project.join(directory)).unwrap();
    }
    let lua = project.join("src/client/example.lua");
    fs::write(&lua, "return true\n").unwrap();
    fs::write(project.join("src/shared/example.lua"), "return 'shared'\n").unwrap();
    fs::write(
        project.join("src/media/sandbox-options.txt"),
        "VERSION = 1\n",
    )
    .unwrap();

    assert!(km(&["--project", path(&project), "build"]).status.success());
    assert!(
        project
            .join("dist/dev/ReleaseMod/42/media/lua/client/example.lua")
            .is_file()
    );
    assert!(
        project
            .join("dist/dev/ReleaseMod/42/media/lua/shared/example.lua")
            .is_file()
    );
    assert!(
        project
            .join("dist/dev/ReleaseMod/42/media/sandbox-options.txt")
            .is_file()
    );
    assert!(
        km(&["--project", path(&project), "package"])
            .status
            .success()
    );
    assert_eq!(
        fs::read_to_string(project.join(
            "dist/workshop/ReleaseMod/Contents/mods/ReleaseMod/42/media/lua/client/example.lua",
        ),)
        .unwrap(),
        "return true\n"
    );
    assert!(
        project
            .join("dist/workshop/ReleaseMod/Contents/mods/ReleaseMod/CHANGELOG.md")
            .is_file()
    );
    assert!(
        project
            .join("dist/workshop/ReleaseMod/Contents/mods/ReleaseMod/LICENSE")
            .is_file()
    );
    for _ in 0..2 {
        assert!(
            km(&[
                "--project",
                path(&project),
                "install",
                "--root",
                path(&mods),
            ])
            .status
            .success()
        );
    }
}

#[test]
fn handles_scaffold_and_output_edge_cases() {
    let temporary = tempdir().unwrap();
    let project = temporary.path().join("edge-mod");
    assert!(
        km(&["new", path(&project), "--author", "Tester"])
            .status
            .success()
    );
    assert!(!km(&["new", path(&project)]).status.success());
    assert!(
        !km(&["new", path(&temporary.path().join("bad")), "--id", "bad-id"])
            .status
            .success()
    );

    let quiet = km(&["--quiet", "--project", path(&project), "check"]);
    assert!(quiet.status.success());
    assert!(quiet.stderr.is_empty());

    let colored = km(&[
        "--color",
        "always",
        "--project",
        path(&temporary.path().join("missing")),
        "check",
    ]);
    assert!(
        String::from_utf8(colored.stderr)
            .unwrap()
            .contains("\u{1b}[31merror:")
    );

    let plain = km(&[
        "--color",
        "never",
        "--project",
        path(&temporary.path().join("missing")),
        "check",
    ]);
    assert!(
        String::from_utf8(plain.stderr)
            .unwrap()
            .starts_with("error:")
    );

    assert!(km(&["--project", path(&project), "clean"]).status.success());
    assert!(km(&["--project", path(&project), "clean"]).status.success());
}

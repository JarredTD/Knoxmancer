use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::tempdir;

fn km(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_km"))
        .args(arguments)
        .output()
        .expect("km command should run")
}

fn path(path: &Path) -> &str {
    path.to_str().expect("temporary path should be UTF-8")
}

#[test]
fn scaffolds_checks_builds_packages_installs_and_cleans() {
    let temporary = tempdir().unwrap();
    let project = temporary.path().join("example-mod");
    let mods = temporary.path().join("mods");

    assert!(
        km(&["new", path(&project), "--author", "Test Author"])
            .status
            .success()
    );
    assert!(km(&["--project", path(&project), "check"]).status.success());
    assert!(km(&["--project", path(&project), "build"]).status.success());
    assert!(
        km(&["--project", path(&project), "build", "--release"])
            .status
            .success()
    );
    assert!(
        km(&["--project", path(&project), "package"])
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

    assert!(project.join("dist/dev/ExampleMod/42/mod.info").is_file());
    assert!(
        project
            .join("dist/workshop/ExampleMod/Contents/mods/ExampleMod/42/mod.info")
            .is_file()
    );
    assert!(mods.join("ExampleMod/42/mod.info").is_file());

    assert!(km(&["--project", path(&project), "clean"]).status.success());
    assert!(!project.join("dist").exists());
}

#[test]
fn initializes_an_existing_mod_without_rewriting_metadata() {
    let temporary = tempdir().unwrap();
    let metadata = temporary.path().join("src/42/mod.info");
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
fn emits_versioned_json_errors() {
    let temporary = tempdir().unwrap();
    let output = km(&[
        "--format",
        "json",
        "--project",
        path(temporary.path()),
        "check",
    ]);
    assert!(!output.status.success());
    let event: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(event["schema_version"], 1);
    assert_eq!(event["status"], "error");
    assert_eq!(event["kind"], "project");
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
    assert!(help.contains("Creates a verified Steam Workshop directory tree"));
}

#[test]
fn diagnoses_the_environment_without_a_project() {
    let output = km(&["doctor"]);
    assert!(output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("Knoxmancer environment"));
    assert!(stderr.contains("Local mods:"));
}

#[test]
fn reports_validation_failures_and_recovers_after_correction() {
    let temporary = tempdir().unwrap();
    let project = temporary.path().join("validation-mod");
    assert!(
        km(&["new", path(&project), "--author", "Tester"])
            .status
            .success()
    );

    let changelog = project.join("CHANGELOG.md");
    let original_changelog = fs::read_to_string(&changelog).unwrap();
    fs::write(&changelog, "# Changelog\n\n## 0.0.9\n\n- Old.\n").unwrap();
    let mismatch = km(&["--project", path(&project), "check"]);
    assert!(!mismatch.status.success());
    assert!(
        String::from_utf8(mismatch.stderr)
            .unwrap()
            .contains("expected 0.1.0")
    );
    fs::write(&changelog, original_changelog).unwrap();

    let preview = project.join("public/preview.png");
    let original_preview = fs::read(&preview).unwrap();
    fs::write(&preview, b"not a png").unwrap();
    assert!(!km(&["--project", path(&project), "check"]).status.success());
    fs::write(&preview, original_preview).unwrap();

    let translation = project.join("src/42/media/lua/shared/Translate/EN/UI.json");
    fs::create_dir_all(translation.parent().unwrap()).unwrap();
    fs::write(&translation, "[]").unwrap();
    assert!(!km(&["--project", path(&project), "check"]).status.success());
    fs::write(&translation, "{}").unwrap();
    assert!(km(&["--project", path(&project), "check"]).status.success());
}

#[test]
fn executes_configured_test_commands_and_propagates_failures() {
    let temporary = tempdir().unwrap();
    let project = temporary.path().join("test-mod");
    assert!(
        km(&["new", path(&project), "--author", "Tester"])
            .status
            .success()
    );
    let manifest = project.join("knoxmancer.toml");
    let source = fs::read_to_string(&manifest).unwrap();
    let executable = path(Path::new(env!("CARGO_BIN_EXE_km"))).replace('\\', "/");
    fs::write(
        &manifest,
        source.replace(
            "command = [\n    \"lua5.1\",\n    \"tests/run.lua\",\n]",
            &format!("command = [\"{executable}\", \"--version\"]"),
        ),
    )
    .unwrap();
    assert!(km(&["--project", path(&project), "test"]).status.success());

    let source = fs::read_to_string(&manifest).unwrap();
    fs::write(
        &manifest,
        source.replace("\"--version\"", "\"unknown-command\""),
    )
    .unwrap();
    assert!(!km(&["--project", path(&project), "test"]).status.success());
}

#[test]
fn supports_minification_and_replacing_existing_artifacts() {
    let temporary = tempdir().unwrap();
    let project = temporary.path().join("release-mod");
    let mods = temporary.path().join("mods");
    assert!(
        km(&["new", path(&project), "--author", "Tester"])
            .status
            .success()
    );
    let lua = project.join("src/42/media/lua/client/example.lua");
    fs::write(&lua, "return true\n").unwrap();

    let manifest = project.join("knoxmancer.toml");
    let mut source = fs::read_to_string(&manifest).unwrap();
    #[cfg(windows)]
    source.push_str(
        "\n[release.minify]\ncommand = \"cmd\"\nargs = [\"/c\", \"copy\", \"/y\", \"{input}\", \"{output}\"]\n",
    );
    #[cfg(unix)]
    source.push_str("\n[release.minify]\ncommand = \"cp\"\nargs = [\"{input}\", \"{output}\"]\n");
    fs::write(&manifest, source).unwrap();

    assert!(
        km(&["--project", path(&project), "build", "--release"])
            .status
            .success()
    );
    assert!(
        project
            .join("dist/release/ReleaseMod/42/media/lua/client/example.lua")
            .is_file()
    );
    for _ in 0..2 {
        assert!(
            km(&[
                "--project",
                path(&project),
                "install",
                "--release",
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

    let json = km(&["--format", "json", "--project", path(&project), "check"]);
    assert!(json.status.success());
    let event: serde_json::Value = serde_json::from_slice(&json.stdout).unwrap();
    assert_eq!(event["schema_version"], 1);
    assert_eq!(event["status"], "ok");

    let verbose = km(&["--verbose", "--project", path(&project), "build"]);
    assert!(verbose.status.success());
    assert!(
        String::from_utf8(verbose.stderr)
            .unwrap()
            .contains("Staging artifact")
    );

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

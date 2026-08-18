use std::collections::BTreeSet;
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

    let playable_files = BTreeSet::from([
        "42/media/lua/client/.gitkeep".to_owned(),
        "42/media/lua/server/.gitkeep".to_owned(),
        "42/media/lua/shared/.gitkeep".to_owned(),
        "42/media/scripts/.gitkeep".to_owned(),
        "42/media/textures/.gitkeep".to_owned(),
        "42/mod.info".to_owned(),
    ]);
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
}

#[test]
fn diagnoses_the_environment_without_a_project() {
    let output = km(&["doctor"]);
    assert!(output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("Knoxmancer environment"));
    assert!(stderr.contains("Local mods:"));
    assert!(stderr.contains("Workshop projects:"));
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

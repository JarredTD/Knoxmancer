//! New-project generation and existing-project adoption.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};

mod naming;
mod templates;

use crate::error::{Error, Result};
use crate::project::config::{Config, MANIFEST_NAME};
use crate::project::preview;
use crate::system::environment::default_author;
use naming::{display_name, mod_id, validate_id, validate_text};

/// Initial semantic version assigned to newly scaffolded mods.
const INITIAL_VERSION: &str = "0.1.0";

#[derive(Debug)]
/// Inputs used to scaffold a new mod project.
pub struct NewProjectOptions {
    /// Empty destination directory to create or populate.
    pub directory: PathBuf,
    /// Optional human-readable mod name override.
    pub name: Option<String>,
    /// Optional game-facing mod identifier override.
    pub id: Option<String>,
    /// Optional author override.
    pub author: Option<String>,
    /// Project Zomboid build directory to generate.
    pub build: String,
}

#[derive(Debug)]
/// Identity and location of a newly scaffolded project.
pub struct NewProjectResult {
    /// Absolute project root.
    pub root: PathBuf,
    /// Resolved human-readable mod name.
    pub name: String,
    /// Generated Project Zomboid build directory.
    pub build: String,
}

/// Creates a complete mod scaffold in an empty destination.
pub fn new_project(options: &NewProjectOptions) -> Result<NewProjectResult> {
    let root = absolute(&options.directory)?;
    ensure_empty_destination(&root)?;

    let slug = root
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| Error::project("project directory must have a valid UTF-8 name"))?;
    let name = options.name.clone().unwrap_or_else(|| display_name(slug));
    let id = options.id.clone().unwrap_or_else(|| mod_id(slug));
    validate_id(&id)?;
    let author = options.author.clone().unwrap_or_else(default_author);
    validate_text("mod name", &name)?;
    validate_text("author", &author)?;

    fs::create_dir_all(&root).map_err(Error::io)?;
    if let Err(error) = write_scaffold(&root, &name, &id, &author, &options.build) {
        if root
            .read_dir()
            .is_ok_and(|mut entries| entries.next().is_some())
        {
            let _ = fs::remove_dir_all(&root);
        }
        return Err(error);
    }

    Ok(NewProjectResult {
        root,
        name,
        build: options.build.clone(),
    })
}

/// Writes a manifest for an existing source-oriented Build 42 project.
pub fn init_project(explicit_root: Option<&Path>, force: bool) -> Result<PathBuf> {
    let root = absolute(explicit_root.unwrap_or(Path::new(".")))?;
    let manifest = root.join(MANIFEST_NAME);
    if manifest.exists() && !force {
        return Err(Error::project(format!(
            "{} already exists; pass --force to replace it",
            manifest.display()
        )));
    }

    if !root.join("src/mod.info").is_file() {
        return Err(Error::project("could not find src/mod.info in the project"));
    }

    let mut config = Config::default();
    config.release.include = ["CHANGELOG.md", "LICENSE"]
        .into_iter()
        .map(PathBuf::from)
        .filter(|path| root.join(path).is_file())
        .collect();
    write_manifest(&root, &config)?;
    Ok(root)
}

/// Writes directories, metadata, assets, and project files for a new scaffold.
fn write_scaffold(root: &Path, name: &str, id: &str, author: &str, build: &str) -> Result<()> {
    let config = Config::default();
    write_manifest(root, &config)?;

    let source = root.join("src");
    let media = source.join("media");
    for directory in [
        source.join("client"),
        source.join("server"),
        source.join("shared"),
        media.join("scripts"),
        media.join("textures"),
        root.join("public"),
    ] {
        fs::create_dir_all(directory).map_err(Error::io)?;
    }
    for keep in [
        source.join("client/.gitkeep"),
        source.join("server/.gitkeep"),
        source.join("shared/.gitkeep"),
        media.join("scripts/.gitkeep"),
        media.join("textures/.gitkeep"),
    ] {
        fs::write(keep, []).map_err(Error::io)?;
    }

    let values = [
        ("name", name),
        ("id", id),
        ("version", INITIAL_VERSION),
        ("author", author),
        ("build", build),
    ];
    fs::write(
        root.join("src/mod.info"),
        templates::render(templates::MOD_INFO, &values),
    )
    .map_err(Error::io)?;
    fs::write(
        root.join("CHANGELOG.md"),
        templates::render(templates::CHANGELOG, &values),
    )
    .map_err(Error::io)?;
    fs::write(
        root.join("README.md"),
        templates::render(templates::README, &values),
    )
    .map_err(Error::io)?;
    fs::write(
        root.join("public/description.md"),
        templates::render(templates::DESCRIPTION, &values),
    )
    .map_err(Error::io)?;
    fs::write(
        root.join("public/workshop.txt"),
        templates::render(templates::WORKSHOP, &values),
    )
    .map_err(Error::io)?;
    fs::write(root.join("public/preview.png"), preview::generate(256, 256)).map_err(Error::io)?;
    fs::write(root.join(".gitignore"), templates::GITIGNORE).map_err(Error::io)?;
    Ok(())
}

/// Serializes a manifest using stable, readable TOML formatting.
fn write_manifest(root: &Path, config: &Config) -> Result<()> {
    let encoded = toml::to_string_pretty(config)
        .map_err(|error| Error::project(format!("could not serialize configuration: {error}")))?;
    fs::write(root.join(MANIFEST_NAME), encoded).map_err(Error::io)
}

/// Accepts a missing or empty directory and rejects every other destination.
fn ensure_empty_destination(root: &Path) -> Result<()> {
    if !root.exists() {
        return Ok(());
    }
    if !root.is_dir() {
        return Err(Error::project(format!(
            "destination is not a directory: {}",
            root.display()
        )));
    }
    if root.read_dir().map_err(Error::io)?.next().is_some() {
        return Err(Error::project(format!(
            "destination is not empty: {}",
            root.display()
        )));
    }
    Ok(())
}

/// Resolves a relative path against the current working directory.
fn absolute(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(env::current_dir().map_err(Error::io)?.join(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn creates_complete_build_42_project() {
        let temporary = tempdir().unwrap();
        let root = temporary.path().join("example-mod");
        write_scaffold(&root, "Example Mod", "ExampleMod", "Author", "42").unwrap_err();

        fs::create_dir(&root).unwrap();
        write_scaffold(&root, "Example Mod", "ExampleMod", "Author", "42").unwrap();
        assert!(root.join("knoxmancer.toml").is_file());
        assert!(root.join("src/mod.info").is_file());
        assert!(root.join("src/client/.gitkeep").is_file());
        assert!(root.join("src/shared/.gitkeep").is_file());
        assert!(root.join("src/server/.gitkeep").is_file());
        assert!(root.join("public/preview.png").metadata().unwrap().len() > 24);
        assert!(!root.join("LICENSE").exists());
        assert_eq!(
            &fs::read(root.join("public/preview.png")).unwrap()[..8],
            b"\x89PNG\r\n\x1a\n"
        );
    }

    #[test]
    fn creates_projects_with_derived_defaults() {
        let temporary = tempdir().unwrap();
        let root = temporary.path().join("derived-project");
        let args = NewProjectOptions {
            directory: root.clone(),
            name: None,
            id: None,
            author: None,
            build: "42".to_owned(),
        };
        new_project(&args).unwrap();
        let metadata = fs::read_to_string(root.join("src/mod.info")).unwrap();
        assert!(metadata.contains("name=Derived Project"));
        assert!(metadata.contains("id=DerivedProject"));
        assert!(new_project(&args).is_err());
    }

    #[test]
    fn rejects_invalid_scaffold_destinations() {
        let temporary = tempdir().unwrap();
        let file = temporary.path().join("file");
        fs::write(&file, "data").unwrap();
        assert!(ensure_empty_destination(&file).is_err());

        let nonempty = temporary.path().join("nonempty");
        fs::create_dir(&nonempty).unwrap();
        fs::write(nonempty.join("file"), "data").unwrap();
        assert!(ensure_empty_destination(&nonempty).is_err());
        assert!(absolute(Path::new("relative")).unwrap().is_absolute());
        assert_eq!(absolute(temporary.path()).unwrap(), temporary.path());
    }

    #[test]
    fn initializes_source_projects_and_force_replaces_manifests() {
        let temporary = tempdir().unwrap();
        fs::create_dir(temporary.path().join("src")).unwrap();
        fs::write(temporary.path().join("src/mod.info"), "id=Example").unwrap();
        fs::write(temporary.path().join("LICENSE"), "license").unwrap();
        init_project(Some(temporary.path()), false).unwrap();
        let manifest = fs::read_to_string(temporary.path().join(MANIFEST_NAME)).unwrap();
        assert!(manifest.contains("source = \"src\""));
        assert!(manifest.contains("\"LICENSE\""));
        assert!(init_project(Some(temporary.path()), false).is_err());
        init_project(Some(temporary.path()), true).unwrap();
    }

    #[test]
    fn rejects_initialization_without_mod_metadata() {
        let temporary = tempdir().unwrap();
        assert!(init_project(Some(temporary.path()), false).is_err());
        assert!(write_manifest(&temporary.path().join("missing"), &Config::default()).is_err());
    }
}

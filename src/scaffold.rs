use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::config::{Config, MANIFEST_NAME};
use crate::error::{Error, Result};
use crate::templates;

const INITIAL_VERSION: &str = "0.1.0";

#[derive(Debug)]
pub struct NewProjectOptions {
    pub directory: PathBuf,
    pub name: Option<String>,
    pub id: Option<String>,
    pub author: Option<String>,
    pub build: String,
}

#[derive(Debug)]
pub struct NewProjectResult {
    pub root: PathBuf,
    pub name: String,
    pub build: String,
}

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

pub fn init_project(explicit_root: Option<&Path>, force: bool) -> Result<PathBuf> {
    let root = absolute(explicit_root.unwrap_or(Path::new(".")))?;
    let manifest = root.join(MANIFEST_NAME);
    if manifest.exists() && !force {
        return Err(Error::project(format!(
            "{} already exists; pass --force to replace it",
            manifest.display()
        )));
    }

    let source = if root.join("src/42/mod.info").is_file() {
        PathBuf::from("src")
    } else if root.join("42/mod.info").is_file() {
        PathBuf::from(".")
    } else {
        return Err(Error::project(
            "could not find src/42/mod.info or 42/mod.info in the project",
        ));
    };

    let mut config = Config::default();
    config.paths.source = source;
    config.release.include = ["CHANGELOG.md", "LICENSE"]
        .into_iter()
        .map(PathBuf::from)
        .filter(|path| root.join(path).is_file())
        .collect();
    if root.join("tests/run.lua").is_file() {
        config.test.command = vec!["lua5.1".to_owned(), "tests/run.lua".to_owned()];
    }
    write_manifest(&root, &config)?;
    Ok(root)
}

fn write_scaffold(root: &Path, name: &str, id: &str, author: &str, build: &str) -> Result<()> {
    let config = Config {
        test: crate::config::TestConfig {
            command: vec!["lua5.1".to_owned(), "tests/run.lua".to_owned()],
        },
        release: crate::config::ReleaseConfig {
            include: vec![PathBuf::from("CHANGELOG.md"), PathBuf::from("LICENSE")],
            minify: None,
        },
        ..Config::default()
    };
    write_manifest(root, &config)?;

    let media = root.join(format!("src/{build}/media"));
    for directory in [
        media.join("lua/client"),
        media.join("lua/server"),
        media.join("lua/shared"),
        media.join("scripts"),
        media.join("textures"),
        root.join("public"),
        root.join("tests"),
        root.join(".github/workflows"),
    ] {
        fs::create_dir_all(directory).map_err(Error::io)?;
    }
    for keep in [
        media.join("lua/client/.gitkeep"),
        media.join("lua/server/.gitkeep"),
        media.join("lua/shared/.gitkeep"),
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
        root.join(format!("src/{build}/mod.info")),
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
    fs::write(root.join("LICENSE"), include_str!("../LICENSE")).map_err(Error::io)?;
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
    write_preview(&root.join("public/preview.png"))?;
    fs::write(root.join("tests/run.lua"), templates::TEST_RUNNER).map_err(Error::io)?;
    fs::write(root.join(".gitignore"), templates::GITIGNORE).map_err(Error::io)?;
    fs::write(root.join(".github/workflows/ci.yml"), templates::CI).map_err(Error::io)?;
    Ok(())
}

fn write_manifest(root: &Path, config: &Config) -> Result<()> {
    let encoded = toml::to_string_pretty(config)
        .map_err(|error| Error::project(format!("could not serialize configuration: {error}")))?;
    fs::write(root.join(MANIFEST_NAME), encoded).map_err(Error::io)
}

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

fn absolute(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(env::current_dir().map_err(Error::io)?.join(path))
    }
}

fn display_name(slug: &str) -> String {
    slug.split(['-', '_'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut characters = part.chars();
            match characters.next() {
                Some(first) => first.to_uppercase().collect::<String>() + characters.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn mod_id(slug: &str) -> String {
    display_name(slug).replace(' ', "")
}

fn validate_id(id: &str) -> Result<()> {
    if id.is_empty()
        || !id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        return Err(Error::project(
            "mod ID must contain only ASCII letters, digits, and underscores",
        ));
    }
    Ok(())
}

fn default_author() -> String {
    let git_name = Command::new("git")
        .args(["config", "user.name"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|name| name.trim().to_owned())
        .filter(|name| !name.is_empty());
    git_name
        .or_else(|| env::var("USERNAME").ok())
        .or_else(|| env::var("USER").ok())
        .unwrap_or_else(|| "Unknown".to_owned())
}

fn write_preview(path: &Path) -> Result<()> {
    let mut raw = Vec::with_capacity(256 * (1 + 256 * 4));
    let mut row = Vec::with_capacity(1 + 256 * 4);
    row.push(0);
    for _ in 0..256 {
        row.extend_from_slice(&[44, 48, 46, 255]);
    }
    for _ in 0..256 {
        raw.extend_from_slice(&row);
    }

    let mut compressed = vec![0x78, 0x01];
    let mut remaining = raw.as_slice();
    while !remaining.is_empty() {
        let length = remaining.len().min(65_535);
        let final_block = length == remaining.len();
        compressed.push(u8::from(final_block));
        compressed.extend_from_slice(&(length as u16).to_le_bytes());
        compressed.extend_from_slice(&(!(length as u16)).to_le_bytes());
        compressed.extend_from_slice(&remaining[..length]);
        remaining = &remaining[length..];
    }
    compressed.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut png = Vec::new();
    png.extend_from_slice(b"\x89PNG\r\n\x1a\n");
    let mut header = Vec::new();
    header.extend_from_slice(&256_u32.to_be_bytes());
    header.extend_from_slice(&256_u32.to_be_bytes());
    header.extend_from_slice(&[8, 6, 0, 0, 0]);
    write_chunk(&mut png, b"IHDR", &header);
    write_chunk(&mut png, b"IDAT", &compressed);
    write_chunk(&mut png, b"IEND", &[]);

    let mut file = fs::File::create(path).map_err(Error::io)?;
    file.write_all(&png).map_err(Error::io)
}

fn write_chunk(output: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    output.extend_from_slice(&(data.len() as u32).to_be_bytes());
    output.extend_from_slice(kind);
    output.extend_from_slice(data);
    let mut checksum_data = Vec::with_capacity(kind.len() + data.len());
    checksum_data.extend_from_slice(kind);
    checksum_data.extend_from_slice(data);
    output.extend_from_slice(&crc32(&checksum_data).to_be_bytes());
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & (0_u32.wrapping_sub(crc & 1)));
        }
    }
    !crc
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1_u32, 0_u32);
    for byte in data {
        a = (a + u32::from(*byte)) % 65_521;
        b = (b + a) % 65_521;
    }
    (b << 16) | a
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
        assert!(root.join("src/42/mod.info").is_file());
        assert!(root.join("public/preview.png").metadata().unwrap().len() > 24);
        assert_eq!(
            &fs::read(root.join("public/preview.png")).unwrap()[..8],
            b"\x89PNG\r\n\x1a\n"
        );
    }

    #[test]
    fn derives_human_and_game_names() {
        assert_eq!(display_name("connected-storage"), "Connected Storage");
        assert_eq!(mod_id("connected-storage"), "ConnectedStorage");
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
        let metadata = fs::read_to_string(root.join("src/42/mod.info")).unwrap();
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
        assert!(validate_id("").is_err());
        assert!(validate_id("valid_ID2").is_ok());
        assert!(absolute(Path::new("relative")).unwrap().is_absolute());
        assert_eq!(absolute(temporary.path()).unwrap(), temporary.path());
    }

    #[test]
    fn initializes_flat_projects_and_force_replaces_manifests() {
        let temporary = tempdir().unwrap();
        fs::create_dir(temporary.path().join("42")).unwrap();
        fs::write(temporary.path().join("42/mod.info"), "id=Example").unwrap();
        fs::write(temporary.path().join("LICENSE"), "license").unwrap();
        init_project(Some(temporary.path()), false).unwrap();
        let manifest = fs::read_to_string(temporary.path().join(MANIFEST_NAME)).unwrap();
        assert!(manifest.contains("source = \".\""));
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

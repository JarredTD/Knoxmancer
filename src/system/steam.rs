//! Steam and Project Zomboid installation discovery.

use std::path::PathBuf;

use steamlocate::SteamDir;

use crate::error::{Error, Result};

/// Steam application identifier for Project Zomboid.
const PROJECT_ZOMBOID_APP_ID: u32 = 108_600;

/// Locates the mods directory read by Project Zomboid's Workshop uploader.
pub(crate) fn project_zomboid_mods_root() -> Result<PathBuf> {
    let steam = SteamDir::locate()
        .map_err(|error| Error::project(format!("could not locate Steam: {error}")))?;
    project_zomboid_mods_root_from(&steam)
}

/// Resolves the uploader mods directory from a known Steam installation.
fn project_zomboid_mods_root_from(steam: &SteamDir) -> Result<PathBuf> {
    let (app, library) = steam
        .find_app(PROJECT_ZOMBOID_APP_ID)
        .map_err(|error| Error::project(format!("could not inspect Steam libraries: {error}")))?
        .ok_or_else(|| Error::project("Project Zomboid is not installed through Steam"))?;
    let game = library.resolve_app_dir(&app);
    if !game.is_dir() {
        return Err(Error::project(format!(
            "Project Zomboid installation is missing: {}",
            game.display()
        )));
    }
    Ok(game.join("mods"))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use tempfile::tempdir;

    /// Writes the minimum Steam metadata needed by the discovery dependency.
    fn steam_fixture(root: &std::path::Path, include_zomboid: bool) -> SteamDir {
        let steamapps = root.join("steamapps");
        let vdf_root = root.display().to_string().replace('\\', "\\\\");
        fs::create_dir_all(&steamapps).unwrap();
        fs::write(
            steamapps.join("libraryfolders.vdf"),
            format!(
                "\"libraryfolders\"\n{{\n  \"0\"\n  {{\n    \"path\" \"{}\"\n  }}\n}}\n",
                vdf_root
            ),
        )
        .unwrap();
        if include_zomboid {
            fs::write(
                steamapps.join("appmanifest_108600.acf"),
                "\"AppState\"\n{\n  \"appid\" \"108600\"\n  \"name\" \"Project Zomboid\"\n  \"installdir\" \"ProjectZomboid\"\n}\n",
            )
            .unwrap();
        }
        SteamDir::from_dir(root).unwrap()
    }

    #[test]
    fn locates_project_zomboid_uploader_directory() {
        let temporary = tempdir().unwrap();
        let game = temporary.path().join("steamapps/common/ProjectZomboid");
        fs::create_dir_all(&game).unwrap();
        let steam = steam_fixture(temporary.path(), true);

        assert_eq!(
            project_zomboid_mods_root_from(&steam).unwrap(),
            game.join("mods")
        );
    }

    #[test]
    fn rejects_missing_project_zomboid_installations() {
        let temporary = tempdir().unwrap();
        let steam = steam_fixture(temporary.path(), false);
        assert!(project_zomboid_mods_root_from(&steam).is_err());

        let steam = steam_fixture(temporary.path(), true);
        assert!(project_zomboid_mods_root_from(&steam).is_err());
    }
}

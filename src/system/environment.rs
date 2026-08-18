//! Host environment discovery.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::error::{Error, Result};

/// Reports Git availability and inferred Zomboid project directories.
pub(crate) fn doctor() -> Vec<String> {
    let mut lines = vec!["Knoxmancer environment".to_owned()];
    lines.push(report_command("git", &["--version"]));

    if let Some(home) = home_directory() {
        let mods = home.join("Zomboid").join("mods");
        let workshop = home.join("Zomboid").join("Workshop");
        lines.push(format!(
            "Local mods: {} ({})",
            mods.display(),
            if mods.is_dir() { "found" } else { "not found" }
        ));
        lines.push(format!(
            "Workshop projects: {} ({})",
            workshop.display(),
            if workshop.is_dir() {
                "found"
            } else {
                "not found"
            }
        ));
    } else {
        lines.push("Local mods: home directory unavailable".to_owned());
        lines.push("Workshop projects: home directory unavailable".to_owned());
    }
    lines
}

/// Resolves the current user's home directory from platform environment variables.
pub(crate) fn home_directory() -> Option<PathBuf> {
    env::var_os("USERPROFILE")
        .or_else(|| env::var_os("HOME"))
        .map(PathBuf::from)
}

/// Resolves an explicit root or a named directory under the user's Zomboid folder.
pub(crate) fn zomboid_root(configured: Option<&Path>, directory: &str) -> Result<PathBuf> {
    match configured {
        Some(path) if path.is_absolute() => Ok(path.to_path_buf()),
        Some(path) => Ok(std::env::current_dir().map_err(Error::io)?.join(path)),
        None => {
            let Some(home) = home_directory() else {
                return Err(Error::project("home directory is unavailable; pass --root"));
            };
            Ok(home.join("Zomboid").join(directory))
        }
    }
}

/// Infers a scaffold author from Git configuration or operating-system identity.
pub(crate) fn default_author() -> String {
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

/// Executes a diagnostic command and formats its first output line.
fn report_command(name: &str, arguments: &[&str]) -> String {
    match Command::new(name).args(arguments).output() {
        Ok(output) if output.status.success() => {
            let text = if output.stdout.is_empty() {
                &output.stderr
            } else {
                &output.stdout
            };
            format!("{name}: {}", String::from_utf8_lossy(text).trim())
        }
        _ => format!("{name}: not found"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_explicit_and_default_zomboid_roots() {
        let absolute = std::env::current_dir().unwrap().join("absolute-root");
        assert_eq!(zomboid_root(Some(&absolute), "mods").unwrap(), absolute);
        assert!(
            zomboid_root(Some(Path::new("relative-root")), "mods")
                .unwrap()
                .is_absolute()
        );
        assert!(
            zomboid_root(None, "Workshop")
                .unwrap()
                .ends_with("Zomboid/Workshop")
        );
    }
}

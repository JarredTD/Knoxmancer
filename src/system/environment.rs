//! Host environment discovery.

use std::env;
use std::path::PathBuf;
use std::process::Command;

/// Reports Git availability and inferred Zomboid project directories.
pub(crate) fn doctor() -> Vec<String> {
    let mut lines = vec!["Knoxmancer environment".to_owned()];
    lines.push(report_command("git", &["--version"]));

    if let Some(home) = home_directory() {
        let mods = home.join("Zomboid/mods");
        let workshop = home.join("Zomboid/Workshop");
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

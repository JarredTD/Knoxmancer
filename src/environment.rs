//! Host environment discovery and external tool inspection.

use std::env;
use std::path::PathBuf;
use std::process::Command;

pub(crate) fn doctor() -> Vec<String> {
    let mut lines = vec!["Knoxmancer environment".to_owned()];
    lines.push(report_command("git", &["--version"]));
    lines.push(report_command("lua5.1", &["-v"]));
    lines.push(report_command("prometheus-lua", &["--version"]));

    if let Some(home) = home_directory() {
        let mods = home.join("Zomboid/mods");
        lines.push(format!(
            "Local mods: {} ({})",
            mods.display(),
            if mods.is_dir() { "found" } else { "not found" }
        ));
    } else {
        lines.push("Local mods: home directory unavailable".to_owned());
    }
    lines
}

pub(crate) fn command_exists(command: &str) -> bool {
    Command::new(command).arg("--version").output().is_ok()
}

pub(crate) fn home_directory() -> Option<PathBuf> {
    env::var_os("USERPROFILE")
        .or_else(|| env::var_os("HOME"))
        .map(PathBuf::from)
}

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

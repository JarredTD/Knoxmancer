//! External Lua minifier execution.

use std::fs;
use std::path::Path;
use std::process::Command;

use walkdir::WalkDir;

use crate::config::MinifyConfig;
use crate::error::{Error, Result};

pub(crate) fn minify_lua(root: &Path, config: &MinifyConfig) -> Result<usize> {
    let files: Vec<_> = WalkDir::new(root)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry.file_type().is_file()
                && entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "lua")
        })
        .map(|entry| entry.into_path())
        .collect();
    for source in &files {
        let generated = source.with_extension("lua.knoxmancer");
        let arguments: Vec<_> = config
            .args
            .iter()
            .map(|argument| {
                argument
                    .replace("{input}", &source.to_string_lossy())
                    .replace("{output}", &generated.to_string_lossy())
            })
            .collect();
        let status = Command::new(&config.command)
            .args(&arguments)
            .status()
            .map_err(|error| Error::tool(format!("could not run {}: {error}", config.command)))?;
        if !status.success() {
            let _ = fs::remove_file(&generated);
            return Err(Error::tool(format!(
                "minifier failed for {}",
                source.display()
            )));
        }
        if config
            .args
            .iter()
            .any(|argument| argument.contains("{output}"))
        {
            if !generated.is_file() || generated.metadata().map_err(Error::io)?.len() == 0 {
                return Err(Error::tool(format!(
                    "minifier produced no output for {}",
                    source.display()
                )));
            }
            fs::rename(&generated, source).map_err(Error::io)?;
        }
    }
    Ok(files.len())
}

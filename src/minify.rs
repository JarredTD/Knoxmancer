//! External Lua minifier execution.

use std::fs;
use std::path::Path;

use walkdir::WalkDir;

use crate::config::MinifyConfig;
use crate::error::{Error, Result};
use crate::process;

#[derive(Debug)]
pub(crate) struct MinifyResult {
    pub files: usize,
    pub output: Vec<String>,
}

pub(crate) fn minify_lua(root: &Path, config: &MinifyConfig) -> Result<MinifyResult> {
    let files: Vec<_> = WalkDir::new(root)
        .into_iter()
        .map(|entry| entry.map_err(|error| Error::io(std::io::Error::other(error))))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .filter(|entry| {
            entry.file_type().is_file()
                && entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "lua")
        })
        .map(|entry| entry.into_path())
        .collect();
    let mut messages = Vec::new();
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
        let output = process::run(&config.command, &arguments, None)?;
        if !output.success() {
            let _ = fs::remove_file(&generated);
            return Err(Error::tool(format!(
                "minifier failed for {} with {}{}",
                source.display(),
                output.status_description(),
                output.failure_detail()
            )));
        }
        messages.extend(output.lines());
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
    Ok(MinifyResult {
        files: files.len(),
        output: messages,
    })
}

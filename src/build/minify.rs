//! External Lua minifier execution.

use std::fs;
use std::path::Path;

use walkdir::WalkDir;

use crate::error::{Error, Result};
use crate::project::config::MinifyConfig;
use crate::system::process;

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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn reports_process_and_output_failures() {
        let temporary = tempdir().unwrap();
        let lua = temporary.path().join("42/media/lua/client/example.lua");
        fs::create_dir_all(lua.parent().unwrap()).unwrap();
        fs::write(&lua, "return true").unwrap();

        let missing = MinifyConfig {
            command: "knoxmancer-command-that-does-not-exist".to_owned(),
            args: Vec::new(),
        };
        assert!(minify_lua(temporary.path(), &missing).is_err());

        #[cfg(windows)]
        let failed = MinifyConfig {
            command: "cmd".to_owned(),
            args: vec!["/c".to_owned(), "exit".to_owned(), "1".to_owned()],
        };
        #[cfg(unix)]
        let failed = MinifyConfig {
            command: "false".to_owned(),
            args: Vec::new(),
        };
        assert!(minify_lua(temporary.path(), &failed).is_err());

        #[cfg(windows)]
        let no_output = MinifyConfig {
            command: "cmd".to_owned(),
            args: vec![
                "/c".to_owned(),
                "exit".to_owned(),
                "0".to_owned(),
                "{output}".to_owned(),
            ],
        };
        #[cfg(unix)]
        let no_output = MinifyConfig {
            command: "true".to_owned(),
            args: vec!["{output}".to_owned()],
        };
        assert!(minify_lua(temporary.path(), &no_output).is_err());
    }
}

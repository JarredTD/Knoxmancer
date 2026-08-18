//! Platform directory launching behind a testable process boundary.

use std::path::Path;
use std::process::Command;

use crate::error::{Error, Result};

/// Process operation needed to launch the platform file browser.
trait Launcher {
    /// Starts the platform command with one directory argument.
    fn launch(&self, program: &str, path: &Path) -> std::io::Result<()>;
}

/// Production child-process launcher.
struct SystemLauncher;

impl Launcher for SystemLauncher {
    fn launch(&self, program: &str, path: &Path) -> std::io::Result<()> {
        Command::new(program).arg(path).spawn().map(|_| ())
    }
}

/// Opens an existing directory in the platform file browser.
pub(crate) fn open(path: &Path) -> Result<()> {
    open_with(&SystemLauncher, path)
}

/// Validates a directory before dispatching to a supplied launcher.
fn open_with(launcher: &impl Launcher, path: &Path) -> Result<()> {
    if !path.is_dir() {
        return Err(Error::project(format!(
            "directory does not exist yet: {}; run the corresponding build, install, package, or stage command first",
            path.display()
        )));
    }
    launcher.launch(platform_program(), path).map_err(Error::io)
}

/// Returns the platform file-browser command.
fn platform_program() -> &'static str {
    #[cfg(windows)]
    return "explorer.exe";

    #[cfg(target_os = "macos")]
    return "open";

    #[cfg(all(unix, not(target_os = "macos")))]
    return "xdg-open";
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use tempfile::tempdir;

    #[derive(Default)]
    struct RecordingLauncher {
        calls: RefCell<Vec<(String, std::path::PathBuf)>>,
    }

    impl Launcher for RecordingLauncher {
        fn launch(&self, program: &str, path: &Path) -> std::io::Result<()> {
            self.calls
                .borrow_mut()
                .push((program.to_owned(), path.to_path_buf()));
            Ok(())
        }
    }

    #[test]
    fn opens_existing_directories_and_rejects_missing_ones() {
        let temporary = tempdir().unwrap();
        let launcher = RecordingLauncher::default();
        open_with(&launcher, temporary.path()).unwrap();
        assert_eq!(launcher.calls.borrow().len(), 1);
        assert_eq!(launcher.calls.borrow()[0].0, platform_program());
        assert!(open_with(&launcher, &temporary.path().join("missing")).is_err());
    }
}

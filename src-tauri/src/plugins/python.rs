//! The Python that runs the plugins written in Python: the one bundled with the app, the same
//! version Ulanzi Studio bundles, so the native modules those plugins carry load in it.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Lets tests and advanced users pick the interpreter.
pub const PYTHON_ENV: &str = "TAURILANZI_PYTHON";
/// Inside the app's resources, as `scripts/fetch-python.ts` leaves it.
const BUNDLED: &str = if cfg!(windows) {
    "python/python.exe"
} else {
    "python/bin/python3"
};

#[derive(Debug, Clone)]
pub struct Python {
    pub path: PathBuf,
    pub version: String,
}

/// `TAURILANZI_PYTHON` if set, then the bundled Python. `resources` is the app's resource folder;
/// in development builds the one `fetch-python` fills in the source tree is used too.
pub fn find(resources: Option<&Path>) -> Option<Python> {
    candidates(resources).into_iter().find_map(|path| {
        let version = version(&path)?;
        Some(Python { path, version })
    })
}

fn candidates(resources: Option<&Path>) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = env::var_os(PYTHON_ENV)
        .map(PathBuf::from)
        .into_iter()
        .collect();
    paths.extend(resources.map(|dir| dir.join(BUNDLED)));
    if cfg!(debug_assertions) {
        paths.push(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("resources")
                .join(BUNDLED),
        );
    }
    paths
}

/// `Python 3.14.10` → `3.14.10`.
fn version(path: &Path) -> Option<String> {
    if !path.is_file() {
        return None;
    }
    let mut command = Command::new(path);
    command.arg("--version").env_remove("PYTHONHOME");
    #[cfg(windows)]
    crate::os::windows::without_console(&mut command);
    let output = command.output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let text = text.trim();
    output
        .status
        .success()
        .then(|| text.strip_prefix("Python ").unwrap_or(text).to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bundled_python_comes_from_the_resources() {
        let dir = tempfile::tempdir().unwrap();
        let paths = candidates(Some(dir.path()));
        assert!(paths.contains(&dir.path().join(BUNDLED)));
        assert!(
            !paths.iter().any(|p| p.to_string_lossy().contains("Ulanzi")),
            "never Ulanzi Studio's"
        );
    }

    #[cfg(unix)]
    #[test]
    fn finds_a_python_that_answers() {
        if env::var_os(PYTHON_ENV).is_some() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let python = dir.path().join(BUNDLED);
        crate::plugins::test_scripts::write_script(&python, "#!/bin/sh\necho Python 3.14.10\n");
        let found = find(Some(dir.path())).unwrap();
        assert_eq!((found.path, found.version.as_str()), (python, "3.14.10"));
    }
}

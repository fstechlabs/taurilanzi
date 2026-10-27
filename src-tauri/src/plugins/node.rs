//! The Node that runs the plugins written in JavaScript: the one bundled with the app, so plugins
//! work the same on every computer whatever is installed on it.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Lets tests and advanced users pick the binary.
pub const NODE_ENV: &str = "TAURILANZI_NODE";
/// Inside the app's resources, as `scripts/fetch-node.ts` leaves it.
const BUNDLED: &str = if cfg!(windows) {
    "node/node.exe"
} else {
    "node/node"
};

#[derive(Debug, Clone)]
pub struct Node {
    pub path: PathBuf,
    pub version: String,
}

/// `TAURILANZI_NODE` if set, then the bundled Node. `resources` is the app's resource folder; in
/// development builds the one `fetch-node` fills in the source tree is used too.
pub fn find(resources: Option<&Path>) -> Option<Node> {
    candidates(resources).into_iter().find_map(|path| {
        let version = version(&path)?;
        Some(Node { path, version })
    })
}

fn candidates(resources: Option<&Path>) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = env::var_os(NODE_ENV)
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

fn version(path: &Path) -> Option<String> {
    if !path.is_file() {
        return None;
    }
    let mut command = Command::new(path);
    command.arg("--version");
    #[cfg(windows)]
    crate::os::windows::without_console(&mut command);
    let output = command.output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bundled_node_comes_from_the_resources() {
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
    fn finds_a_node_that_answers() {
        use std::os::unix::fs::PermissionsExt;

        if env::var_os(NODE_ENV).is_some() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let node = dir.path().join("node/node");
        std::fs::create_dir_all(node.parent().unwrap()).unwrap();
        std::fs::write(&node, "#!/bin/sh\necho v24.0.0\n").unwrap();
        std::fs::set_permissions(&node, std::fs::Permissions::from_mode(0o755)).unwrap();
        let found = find(Some(dir.path())).unwrap();
        assert_eq!((found.path, found.version.as_str()), (node, "v24.0.0"));
    }
}

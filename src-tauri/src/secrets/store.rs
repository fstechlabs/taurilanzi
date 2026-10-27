//! Passwords and tokens: in the system keychain, or in a file of the data folder for tests.

use std::io;
use std::path::{Path, PathBuf};

use super::{file, platform};
use crate::setup::DATA_DIR_ENV;

/// The released app saved its secrets under this service before each build had one of its own.
const RELEASED_SERVICE: &str = "taurilanzi";
const RELEASED_IDENTIFIER: &str = "com.taurilanzi.desktop";
const FILE_NAME: &str = "secrets.json";

#[derive(Debug, Clone)]
pub struct Secrets {
    /// Set when running on a data folder of its own, which must never touch the real keychain.
    file: Option<PathBuf>,
    service: String,
}

impl Secrets {
    /// `identifier` is the app's, so a development build never touches the released app's secrets.
    pub fn new(data_dir: &Path, identifier: &str) -> Self {
        let isolated = cfg!(test) || std::env::var_os(DATA_DIR_ENV).is_some();
        Self {
            file: isolated.then(|| data_dir.join(FILE_NAME)),
            service: match identifier {
                RELEASED_IDENTIFIER => RELEASED_SERVICE.to_owned(),
                other => other.to_owned(),
            },
        }
    }

    pub fn get(&self, account: &str) -> io::Result<Option<String>> {
        match &self.file {
            Some(path) => file::get(path, account),
            None => platform::get(&self.service, account),
        }
    }

    /// Without reading the secret, so the keychain never asks the user for permission.
    pub fn exists(&self, account: &str) -> io::Result<bool> {
        match &self.file {
            Some(path) => file::get(path, account).map(|value| value.is_some()),
            None => platform::exists(&self.service, account),
        }
    }

    /// `None` deletes it.
    pub fn set(&self, account: &str, value: Option<&str>) -> io::Result<()> {
        match &self.file {
            Some(path) => file::set(path, account, value),
            None => platform::set(&self.service, account, value),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tests_never_touch_the_keychain() {
        let dir = tempfile::tempdir().unwrap();
        let secrets = Secrets::new(dir.path(), "com.taurilanzi.test");
        assert_eq!(secrets.file, Some(dir.path().join(FILE_NAME)));
        assert!(!secrets.exists("obs").unwrap());
        secrets.set("obs", Some("hunter2")).unwrap();
        assert!(secrets.exists("obs").unwrap());
        assert_eq!(secrets.get("obs").unwrap().as_deref(), Some("hunter2"));
        assert!(dir.path().join(FILE_NAME).exists());
        secrets.set("obs", None).unwrap();
        assert!(!secrets.exists("obs").unwrap());
        assert_eq!(secrets.get("obs").unwrap(), None);
    }
}

//! Passwords and tokens, in a file of the data folder that only the user can read. The system
//! keychain asked for the user's password again after every update of an unsigned build, so it is
//! only read once, to bring over what earlier versions saved there.

use std::collections::BTreeMap;
use std::io;
use std::path::{Path, PathBuf};

use super::{file, platform};
use crate::setup::DATA_DIR_ENV;

/// The released app saved its secrets under this service before each build had one of its own.
const RELEASED_SERVICE: &str = "taurilanzi";
const RELEASED_IDENTIFIER: &str = "com.taurilanzi.desktop";
const FILE_NAME: &str = "secrets.json";
/// Left once the keychain was read, so it is never read again.
const MIGRATED_MARK: &str = ".keychain-migrated";

#[derive(Debug, Clone)]
pub struct Secrets {
    path: PathBuf,
    /// Where earlier versions kept the secrets in the keychain; none when running on a data
    /// folder of its own, which must never touch the real keychain.
    keychain: Option<String>,
}

impl Secrets {
    /// `identifier` is the app's, so a development build never touches the released app's secrets.
    pub fn new(data_dir: &Path, identifier: &str) -> Self {
        let isolated = cfg!(test) || std::env::var_os(DATA_DIR_ENV).is_some();
        Self {
            path: data_dir.join(FILE_NAME),
            keychain: (!isolated).then(|| match identifier {
                RELEASED_IDENTIFIER => RELEASED_SERVICE.to_owned(),
                other => other.to_owned(),
            }),
        }
    }

    pub fn get(&self, account: &str) -> io::Result<Option<String>> {
        file::get(&self.path, account)
    }

    pub fn exists(&self, account: &str) -> io::Result<bool> {
        file::get(&self.path, account).map(|value| value.is_some())
    }

    /// `None` deletes it.
    pub fn set(&self, account: &str, value: Option<&str>) -> io::Result<()> {
        file::set(&self.path, account, value)
    }

    pub fn all(&self) -> io::Result<BTreeMap<String, String>> {
        file::read_all(&self.path)
    }

    /// Sets every secret of `entries` and deletes the others for which `replaced` holds, in one
    /// write. Returns the accounts whose value changed.
    pub fn replace(
        &self,
        entries: &BTreeMap<String, String>,
        replaced: impl Fn(&str) -> bool,
    ) -> io::Result<Vec<String>> {
        file::update(&self.path, |current| {
            let mut changed: Vec<String> = current
                .iter()
                .filter(|(account, value)| {
                    replaced(account) && entries.get(*account) != Some(*value)
                })
                .map(|(account, _)| account.clone())
                .collect();
            changed.extend(
                entries
                    .iter()
                    .filter(|(account, value)| current.get(*account) != Some(*value))
                    .map(|(account, _)| account.clone()),
            );
            changed.sort();
            changed.dedup();
            current.retain(|account, _| !replaced(account));
            current.extend(entries.clone());
            changed
        })
    }

    /// Moves the secrets of `accounts` from the keychain to the file, the first time only: on
    /// macOS the keychain may ask for the user's password once more for each of them.
    pub fn migrate_from_keychain(&self, accounts: &[String]) {
        let Some(service) = &self.keychain else {
            return;
        };
        let Some(dir) = self.path.parent() else {
            return;
        };
        let mark = dir.join(MIGRATED_MARK);
        if mark.exists() {
            return;
        }
        let mut moved = 0;
        for account in accounts {
            match platform::exists(service, account) {
                Ok(true) => {}
                Ok(false) => continue,
                Err(err) => {
                    log::warn!("could not look for a secret in the keychain: {err}");
                    continue;
                }
            }
            match platform::get(service, account) {
                Ok(Some(value)) => {
                    if let Err(err) = self.set(account, Some(&value)) {
                        log::warn!("could not keep a secret from the keychain: {err}");
                        continue;
                    }
                    moved += 1;
                    if let Err(err) = platform::delete(service, account) {
                        log::warn!("could not delete a secret from the keychain: {err}");
                    }
                }
                Ok(None) => {}
                Err(err) => log::warn!("could not read a secret from the keychain: {err}"),
            }
        }
        if moved > 0 {
            log::info!("{moved} secrets moved from the keychain");
        }
        if let Err(err) = std::fs::write(&mark, b"") {
            log::warn!("could not mark the keychain as read: {err}");
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
        assert_eq!(secrets.keychain, None);
        secrets.migrate_from_keychain(&["obs".into()]);
        assert!(!dir.path().join(MIGRATED_MARK).exists());
    }

    #[test]
    fn sets_gets_and_deletes() {
        let dir = tempfile::tempdir().unwrap();
        let secrets = Secrets::new(dir.path(), "com.taurilanzi.test");
        assert!(!secrets.exists("obs").unwrap());
        secrets.set("obs", Some("hunter2")).unwrap();
        assert!(secrets.exists("obs").unwrap());
        assert_eq!(secrets.get("obs").unwrap().as_deref(), Some("hunter2"));
        assert!(dir.path().join(FILE_NAME).exists());
        secrets.set("obs", None).unwrap();
        assert!(!secrets.exists("obs").unwrap());
        assert_eq!(secrets.get("obs").unwrap(), None);
    }

    #[test]
    fn replaces_only_what_it_owns() {
        let dir = tempfile::tempdir().unwrap();
        let secrets = Secrets::new(dir.path(), "com.taurilanzi.test");
        secrets.set("obs:a", Some("old")).unwrap();
        secrets.set("obs:gone", Some("x")).unwrap();
        secrets.set("own", Some("kept")).unwrap();
        let entries = BTreeMap::from([
            ("obs:a".to_string(), "new".to_string()),
            ("obs:b".to_string(), "added".to_string()),
        ]);
        let changed = secrets.replace(&entries, |a| a != "own").unwrap();
        assert_eq!(changed, ["obs:a", "obs:b", "obs:gone"]);
        let all = secrets.all().unwrap();
        assert_eq!(all.get("obs:a").map(String::as_str), Some("new"));
        assert_eq!(all.get("obs:b").map(String::as_str), Some("added"));
        assert_eq!(all.get("own").map(String::as_str), Some("kept"));
        assert!(!all.contains_key("obs:gone"));
        assert!(
            secrets
                .replace(&entries, |a| a != "own")
                .unwrap()
                .is_empty()
        );
    }
}

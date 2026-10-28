//! Secrets as a JSON file that only the user can read.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;
use std::sync::{Mutex, PoisonError};

type Entries = BTreeMap<String, String>;

/// The clients of OBS and Home Assistant, the engine and sync each write from their own thread.
static WRITING: Mutex<()> = Mutex::new(());

pub fn get(path: &Path, account: &str) -> io::Result<Option<String>> {
    Ok(read_all(path)?.remove(account))
}

pub fn set(path: &Path, account: &str, value: Option<&str>) -> io::Result<()> {
    update(path, |entries| {
        match value {
            Some(value) => entries.insert(account.to_string(), value.to_string()),
            None => entries.remove(account),
        };
    })
}

pub fn read_all(path: &Path) -> io::Result<Entries> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(io::Error::other),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(Entries::new()),
        Err(err) => Err(err),
    }
}

/// Reads, changes and writes the file under one lock, so writers never undo each other.
pub fn update<T>(path: &Path, change: impl FnOnce(&mut Entries) -> T) -> io::Result<T> {
    let _writing = WRITING.lock().unwrap_or_else(PoisonError::into_inner);
    let mut entries = read_all(path)?;
    let result = change(&mut entries);
    write(path, &entries)?;
    Ok(result)
}

/// Through a temporary file, so a reader never finds half of it.
fn write(path: &Path, entries: &Entries) -> io::Result<()> {
    let json = serde_json::to_vec_pretty(entries).map_err(io::Error::other)?;
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("secrets.json");
    let tmp = path.with_file_name(format!(".{name}.tmp"));
    fs::write(&tmp, json)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600))?;
    }
    fs::rename(tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sets_gets_and_deletes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secrets.json");
        assert_eq!(get(&path, "obs").unwrap(), None);
        set(&path, "obs", Some("hunter2")).unwrap();
        set(&path, "other", Some("x")).unwrap();
        assert_eq!(get(&path, "obs").unwrap().as_deref(), Some("hunter2"));
        set(&path, "obs", Some("changed")).unwrap();
        assert_eq!(get(&path, "obs").unwrap().as_deref(), Some("changed"));
        set(&path, "obs", None).unwrap();
        assert_eq!(get(&path, "obs").unwrap(), None);
        assert_eq!(get(&path, "other").unwrap().as_deref(), Some("x"));
        set(&path, "missing", None).unwrap();
    }

    #[test]
    fn a_damaged_file_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secrets.json");
        fs::write(&path, "not json").unwrap();
        assert!(get(&path, "obs").is_err());
        assert!(set(&path, "obs", Some("x")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn only_the_user_can_read_it() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secrets.json");
        set(&path, "obs", Some("hunter2")).unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}

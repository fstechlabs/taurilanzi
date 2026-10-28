//! What this computer remembers of its syncs, in a file of the data folder.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

const FILE_NAME: &str = "cloud.json";

/// A document as it was after the last sync with the cloud.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Base {
    pub version: Uuid,
    /// Of the document as this computer had it then; none when it was deleted.
    pub hash: Option<String>,
    pub edited_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SyncState {
    pub device_id: Option<Uuid>,
    /// Neither uploads nor downloads anything while set.
    pub paused: bool,
    /// The first sync with the account, which may need the user to choose, is done.
    pub joined: bool,
    pub documents: BTreeMap<Uuid, Base>,
    /// Profiles of the cloud deleted on this computer only, with their names, which stay out.
    pub ignored: BTreeMap<Uuid, String>,
    pub last_sync: Option<String>,
}

pub struct StateFile {
    path: PathBuf,
}

impl StateFile {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            path: data_dir.join(FILE_NAME),
        }
    }

    /// Starts over when the file is missing or unreadable: the next sync compares everything.
    pub fn load(&self) -> SyncState {
        fs::read(&self.path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, state: &SyncState) -> io::Result<()> {
        let json = serde_json::to_vec_pretty(state).map_err(io::Error::other)?;
        let tmp = self.path.with_file_name(format!(".{FILE_NAME}.tmp"));
        fs::write(&tmp, json)?;
        fs::rename(tmp, &self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_what_it_remembers_and_starts_over_without_it() {
        let dir = tempfile::tempdir().unwrap();
        let file = StateFile::new(dir.path());
        assert!(file.load().documents.is_empty());
        let mut state = SyncState {
            paused: true,
            joined: true,
            ..SyncState::default()
        };
        state.documents.insert(
            Uuid::nil(),
            Base {
                version: Uuid::from_u128(7),
                hash: Some("abc".into()),
                edited_at: "2026-01-01T00:00:00.000Z".into(),
            },
        );
        state.ignored.insert(Uuid::from_u128(3), "Fuera".into());
        file.save(&state).unwrap();
        let loaded = file.load();
        assert!(loaded.paused && loaded.joined);
        assert_eq!(loaded.documents, state.documents);
        assert_eq!(loaded.ignored, state.ignored);

        fs::write(dir.path().join(FILE_NAME), "no es json").unwrap();
        assert!(
            !file.load().joined,
            "a damaged file compares everything again"
        );
    }
}

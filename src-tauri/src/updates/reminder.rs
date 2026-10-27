//! "Remind me later" on an update: the window does not offer it again for a week.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

const FILE_NAME: &str = "update-reminder.json";
const LATER: Duration = Duration::from_secs(7 * 24 * 60 * 60);

#[derive(Serialize, Deserialize)]
struct Saved {
    /// Seconds since the Unix epoch.
    remind_after: u64,
}

pub struct Reminder {
    file: PathBuf,
    after: Mutex<Option<SystemTime>>,
}

impl Reminder {
    pub fn load(data_dir: &Path) -> Self {
        let file = data_dir.join(FILE_NAME);
        let after = fs::read(&file)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Saved>(&bytes).ok())
            .map(|saved| UNIX_EPOCH + Duration::from_secs(saved.remind_after));
        Self {
            file,
            after: Mutex::new(after),
        }
    }

    pub fn postponed(&self) -> bool {
        self.postponed_at(SystemTime::now())
    }

    fn postponed_at(&self, now: SystemTime) -> bool {
        self.after
            .lock()
            .expect("update reminder")
            .is_some_and(|after| now < after)
    }

    pub fn postpone(&self) -> io::Result<()> {
        let after = SystemTime::now() + LATER;
        *self.after.lock().expect("update reminder") = Some(after);
        let saved = Saved {
            remind_after: after
                .duration_since(UNIX_EPOCH)
                .map_err(io::Error::other)?
                .as_secs(),
        };
        fs::write(&self.file, serde_json::to_vec(&saved)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_postponed_update_waits_a_week_even_after_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let reminder = Reminder::load(dir.path());
        assert!(!reminder.postponed());
        reminder.postpone().unwrap();
        assert!(reminder.postponed());

        let restarted = Reminder::load(dir.path());
        assert!(restarted.postponed());
        assert!(restarted.postponed_at(SystemTime::now() + LATER - Duration::from_secs(60)));
        assert!(!restarted.postponed_at(SystemTime::now() + LATER + Duration::from_secs(60)));
    }
}

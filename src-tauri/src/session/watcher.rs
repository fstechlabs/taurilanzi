//! Tells when the screen locks and unlocks, by asking the system every second.

use std::thread;
use std::time::Duration;

use super::platform;

const POLL: Duration = Duration::from_secs(1);

/// Calls `changed` with each new lock state, starting with `true` if the screen is already
/// locked, until it returns false.
pub fn watch(changed: impl Fn(bool) -> bool + Send + 'static) -> std::io::Result<()> {
    thread::Builder::new()
        .name("session".into())
        .spawn(move || {
            let mut locked = false;
            loop {
                let now = platform::screen_locked().unwrap_or(false);
                if now != locked {
                    locked = now;
                    log::info!("screen {}", if locked { "locked" } else { "unlocked" });
                    if !changed(locked) {
                        return;
                    }
                }
                thread::sleep(POLL);
            }
        })
        .map(|_| ())
}

//! Tells when another app comes to the front, by asking the system twice a second.

use std::thread;
use std::time::Duration;

use super::platform;
use crate::config::AppRef;

const POLL: Duration = Duration::from_millis(500);

/// Calls `changed` with each app that comes to the front, starting with the one there now, until
/// it returns false. This app is never reported, so working in its window does not move the dock;
/// coming back from it reports the app the user returns to, even if it was the last one. Some
/// desktops name the app in front without its process: `own_id` is this app's identifier, which
/// names it there.
pub fn watch(
    own_id: &str,
    changed: impl Fn(AppRef) -> bool + Send + 'static,
) -> std::io::Result<()> {
    let own = std::process::id();
    let own_id = own_id.to_owned();
    thread::Builder::new()
        .name("foreground".into())
        .spawn(move || {
            let mut last: Option<(u32, Option<String>)> = None;
            loop {
                if let Some(mut front) = platform::frontmost() {
                    if front.app.as_ref().is_some_and(|app| app.id == own_id) {
                        front.pid = own;
                    }
                    let now = (front.pid, front.app.as_ref().map(|app| app.id.clone()));
                    if last.as_ref() != Some(&now) {
                        last = Some(now);
                        if front.pid != own
                            && let Some(app) = front.app
                            && !changed(app)
                        {
                            return;
                        }
                    }
                }
                thread::sleep(POLL);
            }
        })
        .map(|_| ())
}

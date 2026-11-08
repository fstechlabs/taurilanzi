//! What Stream Deck plugins hear of the computer: the apps their manifest names starting and
//! quitting, and the system waking up from sleep.

use std::collections::HashSet;
use std::sync::{Arc, PoisonError, RwLock};
use std::thread;
use std::time::{Duration, SystemTime};

use crate::plugins::hub::PluginHost;
use crate::plugins::manifest::Protocol;
use crate::plugins::registry::Registry;

const POLL: Duration = Duration::from_secs(2);
/// A pause of the clock this much longer than the poll is the computer having slept: nothing
/// runs meanwhile, on any system.
const SLEPT: Duration = Duration::from_secs(30);

pub fn watch(host: PluginHost, registry: Arc<RwLock<Registry>>) -> std::io::Result<()> {
    thread::Builder::new()
        .name("plugin system events".into())
        .spawn(move || {
            let mut last = SystemTime::now();
            loop {
                thread::sleep(POLL);
                let now = SystemTime::now();
                if now.duration_since(last).is_ok_and(|gap| gap > POLL + SLEPT) {
                    host.system_woke();
                }
                last = now;
                if monitors_apps(&registry) {
                    let running: HashSet<String> = crate::foreground::running_apps()
                        .into_iter()
                        .map(|app| app.id.to_lowercase())
                        .collect();
                    host.apps_running(running);
                }
            }
        })
        .map(|_| ())
}

fn monitors_apps(registry: &RwLock<Registry>) -> bool {
    registry
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .plugins()
        .iter()
        .any(|p| {
            p.protocol == Protocol::StreamDeck
                && !p.manifest.applications_to_monitor.here().is_empty()
        })
}

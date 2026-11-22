//! Finds the docks plugged in, whatever their family, and starts a worker for each one.

use std::collections::HashSet;
use std::ffi::CString;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use hidapi::HidApi;
use log::{debug, info, warn};

use super::catalog;
use super::dock::{Dock, DockEvent, Event, Reporter, next_connection};
use super::model::Family;
use super::ulanzi;

const SCAN_INTERVAL: Duration = Duration::from_secs(1);

/// Watches for docks on a thread of its own until nobody listens to `events`.
pub fn watch(events: Sender<DockEvent>) -> std::io::Result<()> {
    thread::Builder::new()
        .name("docks".into())
        .spawn(move || scan(events))
        .map(|_| ())
}

fn scan(events: Sender<DockEvent>) {
    let mut api = match HidApi::new() {
        Ok(api) => api,
        Err(err) => {
            warn!("hidapi unavailable: {err}");
            return;
        }
    };
    let open: Arc<Mutex<HashSet<CString>>> = Arc::default();
    loop {
        if let Err(err) = api.refresh_devices() {
            warn!("hid enumeration failed: {err}");
        }
        let found: Vec<_> = api
            .device_list()
            .filter_map(|info| {
                catalog::matching(info.vendor_id(), info.product_id(), info.interface_number())
                    .map(|model| (model, info.path().to_owned(), info.clone()))
            })
            .collect();
        for (model, path, info) in found {
            if lock(&open).contains(&path) {
                continue;
            }
            let device = match info.open_device(&api) {
                Ok(device) => device,
                Err(err) => {
                    debug!("{} found but could not be opened: {err}", model.name);
                    continue;
                }
            };
            let reporter = Reporter::new(next_connection(), events.clone());
            let (dock, requests) = Dock::channel();
            let attached = Event::Attached {
                model,
                dock,
                simulated: false,
            };
            if !reporter.send(attached) {
                return;
            }
            info!("{} found", model.name);
            lock(&open).insert(path.clone());
            let open = Arc::clone(&open);
            let spawned = thread::Builder::new()
                .name(format!("dock-{}", reporter.connection))
                .spawn(move || {
                    match model.family {
                        Family::Ulanzi => ulanzi::run(&device, requests, &reporter),
                    }
                    reporter.send(Event::Disconnected);
                    info!("{} disconnected", model.name);
                    lock(&open).remove(&path);
                });
            if let Err(err) = spawned {
                warn!("could not start the worker of {}: {err}", model.name);
            }
        }
        thread::sleep(SCAN_INTERVAL);
    }
}

fn lock(open: &Mutex<HashSet<CString>>) -> std::sync::MutexGuard<'_, HashSet<CString>> {
    open.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

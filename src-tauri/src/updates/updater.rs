use std::path::Path;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Manager};
use tauri_plugin_updater::{Error, Update, UpdaterExt};
use tauri_specta::Event;

use super::reminder::Reminder;
use crate::ipc::UpdateOffered;
use crate::state::AppState;
use crate::tray;

const FIRST_CHECK: Duration = Duration::from_secs(30);
const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    Idle,
    Checking,
    UpToDate,
    /// No update server in the configuration of this build.
    NotConfigured,
    Downloading(String),
    /// Downloaded and verified, waiting for the user to install it.
    Ready(String),
    Installing(String),
    Failed,
}

/// The update the window offers to install, or the one being installed.
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct UpdateOffer {
    pub version: String,
    pub installing: bool,
}

pub struct Updates {
    status: Mutex<Option<Status>>,
    pending: Mutex<Option<(Update, Vec<u8>)>>,
    reminder: Reminder,
}

impl Updates {
    pub fn new(data_dir: &Path) -> Self {
        Self {
            status: Mutex::default(),
            pending: Mutex::default(),
            reminder: Reminder::load(data_dir),
        }
    }

    pub fn status(&self) -> Status {
        self.status
            .lock()
            .expect("update status")
            .clone()
            .unwrap_or(Status::Idle)
    }

    fn set(&self, status: Status) {
        *self.status.lock().expect("update status") = Some(status);
    }

    pub fn offer(&self) -> Option<UpdateOffer> {
        match self.status() {
            Status::Ready(version) if !self.reminder.postponed() => Some(UpdateOffer {
                version,
                installing: false,
            }),
            Status::Installing(version) => Some(UpdateOffer {
                version,
                installing: true,
            }),
            _ => None,
        }
    }
}

/// Checks soon after start and then every few hours. Only the installed app updates itself.
pub fn schedule(app: AppHandle) {
    if app.state::<AppState>().placement.login_item().is_none() {
        return;
    }
    thread::spawn(move || {
        thread::sleep(FIRST_CHECK);
        loop {
            check(&app);
            thread::sleep(CHECK_EVERY);
        }
    });
}

pub fn check(app: &AppHandle) {
    let state = app.state::<AppState>();
    match state.updates.status() {
        Status::Checking | Status::Downloading(_) | Status::Installing(_) => return,
        // Offered again once a reminder is over.
        Status::Ready(_) => {
            changed(app);
            return;
        }
        _ => {}
    }
    state.updates.set(Status::Checking);
    changed(app);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let status = download(&app).await;
        app.state::<AppState>().updates.set(status);
        changed(&app);
    });
}

/// Tells the menu bar and the window.
fn changed(app: &AppHandle) {
    tray::refresh(app);
    let offer = app.state::<AppState>().updates.offer();
    if let Err(err) = UpdateOffered(offer).emit(app) {
        log::warn!("could not tell the window about the update: {err}");
    }
}

async fn download(app: &AppHandle) -> Status {
    let updater = match app.updater() {
        Ok(updater) => updater,
        Err(Error::EmptyEndpoints) => return Status::NotConfigured,
        Err(err) => {
            log::warn!("updater unavailable: {err}");
            return Status::Failed;
        }
    };
    let update = match updater.check().await {
        Ok(Some(update)) => update,
        Ok(None) => return Status::UpToDate,
        Err(Error::EmptyEndpoints) => return Status::NotConfigured,
        Err(err) => {
            log::warn!("update check failed: {err}");
            return Status::Failed;
        }
    };
    let version = update.version.clone();
    let state = app.state::<AppState>();
    state.updates.set(Status::Downloading(version.clone()));
    changed(app);
    // The signature is verified as part of the download.
    match update.download(|_, _| {}, || {}).await {
        Ok(bytes) => {
            log::info!("update {version} downloaded");
            *state.updates.pending.lock().expect("pending update") = Some((update, bytes));
            Status::Ready(version)
        }
        Err(err) => {
            log::warn!("update download failed: {err}");
            Status::Failed
        }
    }
}

/// Replaces the app and starts it again, away from the caller's thread so the menu bar and the
/// window can show it is installing. The dock goes blank for about a second.
pub fn install(app: &AppHandle) {
    let app = app.clone();
    thread::spawn(move || install_now(&app));
}

fn install_now(app: &AppHandle) {
    let state = app.state::<AppState>();
    let Some((update, bytes)) = state.updates.pending.lock().expect("pending update").take() else {
        return;
    };
    state
        .updates
        .set(Status::Installing(update.version.clone()));
    changed(app);
    match update.install(bytes) {
        Ok(()) => {
            log::info!("update {} installed, restarting", update.version);
            app.restart();
        }
        Err(err) => {
            log::warn!("update install failed: {err}");
            state.updates.set(Status::Failed);
            changed(app);
        }
    }
}

/// Hides the offer for a week; the menu bar still offers the update.
pub fn remind_later(app: &AppHandle) {
    let state = app.state::<AppState>();
    if let Err(err) = state.updates.reminder.postpone() {
        log::warn!("could not save when to remind about the update: {err}");
    }
    changed(app);
}

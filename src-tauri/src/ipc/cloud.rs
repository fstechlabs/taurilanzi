//! The cloud: signing this computer in and out, and how its sync is doing. Changes of the status
//! also come as `CloudChanged` events.

use tauri::State;
use uuid::Uuid;

use super::error::{ErrorKind, IpcError};
use crate::cloud::{Cloud, CloudError, CloudStatus, JoinMode, SignInError};
use crate::i18n::{self, Language};

type Result<T> = std::result::Result<T, IpcError>;

impl From<CloudError> for IpcError {
    fn from(err: CloudError) -> Self {
        let (kind, message) = match &err {
            CloudError::Busy => (ErrorKind::Conflict, tr!("cloud.errors.busy").to_string()),
            CloudError::SignIn(SignInError::Cancelled) => (
                ErrorKind::Conflict,
                tr!("cloud.errors.cancelled").to_string(),
            ),
            CloudError::SignIn(SignInError::Denied) => {
                (ErrorKind::Invalid, tr!("cloud.errors.denied").to_string())
            }
            CloudError::SignIn(SignInError::TimedOut) => (
                ErrorKind::Unavailable,
                tr!("cloud.errors.timedOut").to_string(),
            ),
            CloudError::Api(crate::cloud::ApiError::Network(_)) => (
                ErrorKind::Unavailable,
                tr!("cloud.errors.offline").to_string(),
            ),
            _ => (
                ErrorKind::Failed,
                trf!("cloud.errors.failed", error = err).to_string(),
            ),
        };
        Self::new(kind, message)
    }
}

impl From<std::io::Error> for IpcError {
    fn from(err: std::io::Error) -> Self {
        Self::new(ErrorKind::Failed, err.to_string())
    }
}

fn locale() -> &'static str {
    match i18n::current() {
        Language::Es => "es",
        Language::En => "en",
    }
}

#[tauri::command]
#[specta::specta]
pub fn cloud_status(cloud: State<'_, Cloud>) -> CloudStatus {
    cloud.status()
}

/// Opens the browser to sign in, and answers once this computer is connected.
#[tauri::command]
#[specta::specta]
pub async fn cloud_sign_in(cloud: State<'_, Cloud>) -> Result<()> {
    let cloud = cloud.inner().clone();
    tauri::async_runtime::spawn_blocking(move || cloud.sign_in(locale()))
        .await
        .map_err(|err| IpcError::new(ErrorKind::Failed, err.to_string()))?
        .map_err(Into::into)
}

/// The account's page of the cloud, in the browser: its versions and computers.
#[tauri::command]
#[specta::specta]
pub fn cloud_open_web(cloud: State<'_, Cloud>) -> Result<()> {
    Ok(cloud.open_web(locale())?)
}

#[tauri::command]
#[specta::specta]
pub fn cloud_cancel_sign_in(cloud: State<'_, Cloud>) {
    cloud.cancel_sign_in();
}

/// The profiles stay here as they are, and in the cloud for the other computers.
#[tauri::command]
#[specta::specta]
pub async fn cloud_sign_out(cloud: State<'_, Cloud>) -> Result<()> {
    let cloud = cloud.inner().clone();
    tauri::async_runtime::spawn_blocking(move || cloud.sign_out())
        .await
        .map_err(|err| IpcError::new(ErrorKind::Failed, err.to_string()))?
        .map_err(Into::into)
}

/// Syncs now instead of waiting for the next round.
#[tauri::command]
#[specta::specta]
pub fn cloud_sync_now(cloud: State<'_, Cloud>) {
    cloud.sync_now();
}

#[tauri::command]
#[specta::specta]
pub fn cloud_set_paused(cloud: State<'_, Cloud>, paused: bool) -> Result<()> {
    Ok(cloud.set_paused(paused)?)
}

/// What to do with this computer's profiles, the first time, when the account has some already.
#[tauri::command]
#[specta::specta]
pub fn cloud_join(cloud: State<'_, Cloud>, mode: JoinMode) {
    cloud.join(mode);
}

/// Brings back a profile of the cloud that was deleted on this computer only.
#[tauri::command]
#[specta::specta]
pub fn cloud_bring_back(cloud: State<'_, Cloud>, profile: Uuid) -> Result<()> {
    Ok(cloud.bring_back(profile)?)
}

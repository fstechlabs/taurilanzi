use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tokio::sync::{broadcast, oneshot};
use uuid::Uuid;

use super::commands::{Command, CommandError, Input, KeySlot, PageChanges, Reply};
use super::gestures::Gesture;
use super::snapshot::{EngineEvent, Snapshot};
use crate::clients::Clients;
use crate::config::Integration;
use crate::config::{AppRef, Appearance, DeviceSettings, Key, Position, Preferences};
use crate::hass::Hass;
use crate::obs::Obs;

/// When a key was last pressed. Kept out of the snapshot because it changes all the time.
#[derive(Clone)]
pub struct Activity(Arc<Mutex<Instant>>);

impl Activity {
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(Instant::now())))
    }

    pub fn touch(&self) {
        *self.0.lock().expect("activity") = Instant::now();
    }

    pub fn idle(&self) -> Duration {
        self.0.lock().expect("activity").elapsed()
    }
}

/// Cheap to clone; how the rest of the app talks to the engine thread.
#[derive(Clone)]
pub struct EngineHandle {
    inputs: mpsc::Sender<Input>,
    events: broadcast::Sender<EngineEvent>,
    obs: Clients<Obs>,
    hass: Clients<Hass>,
}

impl EngineHandle {
    pub fn new(
        inputs: mpsc::Sender<Input>,
        events: broadcast::Sender<EngineEvent>,
        obs: Clients<Obs>,
        hass: Clients<Hass>,
    ) -> Self {
        Self {
            inputs,
            events,
            obs,
            hass,
        }
    }

    /// The client of an OBS server, which answers the settings and the editor without going
    /// through the engine thread.
    pub fn obs(&self, server: Uuid) -> Option<Obs> {
        self.obs.get(server)
    }

    /// The client of a Home Assistant server, which answers the settings and the editor without
    /// going through the engine thread.
    pub fn hass(&self, server: Uuid) -> Option<Hass> {
        self.hass.get(server)
    }

    pub async fn save_integration(&self, integration: Integration) -> Result<(), CommandError> {
        self.ask(|reply| Command::SaveIntegration { integration, reply })
            .await?
    }

    pub async fn delete_integration(&self, id: Uuid) -> Result<(), CommandError> {
        self.ask(|reply| Command::DeleteIntegration { id, reply })
            .await?
    }

    pub fn subscribe(&self) -> broadcast::Receiver<EngineEvent> {
        self.events.subscribe()
    }

    pub async fn snapshot(&self) -> Result<Snapshot, CommandError> {
        self.ask(Command::Snapshot).await
    }

    pub async fn set_key(
        &self,
        page: Uuid,
        index: u8,
        key: Option<Key>,
    ) -> Result<(), CommandError> {
        self.ask(|reply| Command::SetKey {
            page,
            index,
            key,
            reply,
        })
        .await?
    }

    pub async fn set_device(&self, settings: DeviceSettings) -> Result<(), CommandError> {
        self.ask(|reply| Command::SetDevice { settings, reply })
            .await?
    }

    pub async fn set_preferences(&self, preferences: Preferences) -> Result<(), CommandError> {
        self.ask(|reply| Command::SetPreferences { preferences, reply })
            .await?
    }

    pub async fn try_screensaver(&self, on: bool) -> Result<(), CommandError> {
        self.ask(|reply| Command::TryScreensaver { on, reply })
            .await
    }

    pub async fn key_image(&self, page: Uuid, index: u8) -> Result<Vec<u8>, CommandError> {
        self.ask(|reply| Command::KeyImage { page, index, reply })
            .await?
    }

    pub async fn preview(
        &self,
        page: Option<Uuid>,
        index: u8,
        appearance: Appearance,
    ) -> Result<Vec<u8>, CommandError> {
        self.ask(|reply| Command::Preview {
            page,
            index,
            appearance,
            reply,
        })
        .await?
    }

    pub async fn fonts(&self) -> Result<Vec<String>, CommandError> {
        self.ask(Command::Fonts).await
    }

    pub async fn set_paused(&self, paused: bool) -> Result<(), CommandError> {
        self.ask(|reply| Command::SetPaused { paused, reply }).await
    }

    pub async fn set_profile(&self, id: Uuid) -> Result<(), CommandError> {
        self.ask(|reply| Command::SetProfile { id, reply }).await?
    }

    pub async fn create_page(
        &self,
        name: Option<String>,
        position: Position,
    ) -> Result<Uuid, CommandError> {
        self.ask(|reply| Command::CreatePage {
            name,
            position,
            reply,
        })
        .await?
    }

    pub async fn update_page(&self, id: Uuid, changes: PageChanges) -> Result<(), CommandError> {
        self.ask(|reply| Command::UpdatePage { id, changes, reply })
            .await?
    }

    pub async fn duplicate_page(&self, id: Uuid) -> Result<Uuid, CommandError> {
        self.ask(|reply| Command::DuplicatePage { id, reply })
            .await?
    }

    pub async fn delete_page(&self, id: Uuid) -> Result<(), CommandError> {
        self.ask(|reply| Command::DeletePage { id, reply }).await?
    }

    pub async fn set_home_page(&self, id: Uuid) -> Result<(), CommandError> {
        self.ask(|reply| Command::SetHomePage { id, reply }).await?
    }

    pub async fn show_page(&self, id: Uuid) -> Result<(), CommandError> {
        self.ask(|reply| Command::ShowPage { id, reply }).await?
    }

    pub async fn move_key(&self, from: KeySlot, to: KeySlot) -> Result<(), CommandError> {
        self.ask(|reply| Command::MoveKey { from, to, reply })
            .await?
    }

    pub async fn press_key(
        &self,
        page: Uuid,
        index: u8,
        gesture: Gesture,
    ) -> Result<(), CommandError> {
        self.ask(|reply| Command::PressKey {
            page,
            index,
            gesture,
            reply,
        })
        .await?
    }

    pub async fn undo(&self) -> Result<(), CommandError> {
        self.ask(Command::Undo).await?
    }

    pub async fn redo(&self) -> Result<(), CommandError> {
        self.ask(Command::Redo).await?
    }

    pub async fn create_profile(&self, name: Option<String>) -> Result<Uuid, CommandError> {
        self.ask(|reply| Command::CreateProfile { name, reply })
            .await?
    }

    pub async fn rename_profile(&self, id: Uuid, name: String) -> Result<(), CommandError> {
        self.ask(|reply| Command::RenameProfile { id, name, reply })
            .await?
    }

    pub async fn duplicate_profile(&self, id: Uuid) -> Result<Uuid, CommandError> {
        self.ask(|reply| Command::DuplicateProfile { id, reply })
            .await?
    }

    pub async fn delete_profile(&self, id: Uuid) -> Result<(), CommandError> {
        self.ask(|reply| Command::DeleteProfile { id, reply })
            .await?
    }

    pub async fn move_profile(&self, id: Uuid, index: usize) -> Result<(), CommandError> {
        self.ask(|reply| Command::MoveProfile { id, index, reply })
            .await?
    }

    pub async fn link_app(&self, profile: Uuid, app: AppRef) -> Result<(), CommandError> {
        self.ask(|reply| Command::LinkApp {
            profile,
            app,
            reply,
        })
        .await?
    }

    pub async fn unlink_app(&self, profile: Uuid, app: String) -> Result<(), CommandError> {
        self.ask(|reply| Command::UnlinkApp {
            profile,
            app,
            reply,
        })
        .await?
    }

    async fn ask<T>(&self, command: impl FnOnce(Reply<T>) -> Command) -> Result<T, CommandError> {
        let (reply, response) = oneshot::channel();
        self.inputs
            .send(Input::Command(command(reply)))
            .map_err(|_| CommandError::EngineStopped)?;
        response.await.map_err(|_| CommandError::EngineStopped)
    }
}

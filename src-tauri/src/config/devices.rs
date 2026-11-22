//! The docks the configuration knows of, connected or not, and the profiles each one owns. A
//! profile belongs to one physical dock; two docks never share or mix their profiles.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::model::{Config, DeviceSettings, Profile};
use crate::driver::FIRST_MODEL;

/// The D200 the app drove before it knew of several docks, which every older configuration had.
/// The same on every computer, so their profiles agree on whose they are before it connects.
pub const FIRST_DOCK: Uuid = Uuid::from_u128(0x0000_0000_0000_4000_8000_0000_0000_d200);

/// A dock seen on some computer of the account, or the first one before it connected.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
pub struct KnownDevice {
    pub id: Uuid,
    /// The id of its model, kept even when this version of the app does not know it.
    pub model: String,
    /// What the user calls it; empty shows the model's name.
    #[serde(default)]
    pub name: String,
    /// What the dock said it is, once one connected; it tells two units of a model apart.
    #[serde(default)]
    pub serial: Option<String>,
    pub settings: DeviceSettings,
}

pub fn first_dock() -> Uuid {
    FIRST_DOCK
}

/// The id of a unit of a model by its serial number: the same on every computer, so one dock met
/// on two of them before they synced is still one dock.
pub fn dock_id(model: &str, serial: &str) -> Uuid {
    hashed(&[b"taurilanzi-dock", model.as_bytes(), serial.as_bytes()])
}

/// An id that follows from two others, the same on every computer: what a dock makes of something
/// of another, as a copy of a profile, gets the same id wherever it is made.
pub fn derived_id(from: Uuid, on: Uuid) -> Uuid {
    hashed(&[b"taurilanzi-derived", from.as_bytes(), on.as_bytes()])
}

fn hashed(parts: &[&[u8]]) -> Uuid {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part);
    }
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&hasher.finalize()[..16]);
    // Version 8 (made by the app) and the RFC variant, so it is a valid UUID.
    bytes[6] = (bytes[6] & 0x0f) | 0x80;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

pub fn first_device(settings: DeviceSettings) -> KnownDevice {
    KnownDevice {
        id: FIRST_DOCK,
        model: FIRST_MODEL.into(),
        name: String::new(),
        serial: None,
        settings,
    }
}

/// Schema 2 had one dock: its settings and the profile on it become the first known dock's.
pub(super) fn migrate(raw: &mut Value) {
    if raw.get("devices").is_some() {
        return;
    }
    let Some(object) = raw.as_object_mut() else {
        return;
    };
    let settings = object
        .remove("device")
        .and_then(|d| serde_json::from_value(d).ok())
        .unwrap_or(DeviceSettings {
            brightness: 70,
            sleep_minutes: 0,
        });
    object.insert("devices".into(), json!([first_device(settings)]));
    if let Some(active) = object.remove("active_profile") {
        object.insert(
            "active_profiles".into(),
            json!({ FIRST_DOCK.to_string(): active }),
        );
    }
}

impl Config {
    pub fn device(&self, id: Uuid) -> Option<&KnownDevice> {
        self.devices.iter().find(|d| d.id == id)
    }

    pub fn device_mut(&mut self, id: Uuid) -> Option<&mut KnownDevice> {
        self.devices.iter_mut().find(|d| d.id == id)
    }

    pub fn profiles_of(&self, device: Uuid) -> impl Iterator<Item = &Profile> {
        self.profiles.iter().filter(move |p| p.device == device)
    }

    /// The profile a dock shows: the one chosen last, or its first.
    pub fn active_profile_of(&self, device: Uuid) -> Option<&Profile> {
        self.active_profiles
            .get(&device)
            .and_then(|id| {
                self.profiles
                    .iter()
                    .find(|p| p.id == *id && p.device == device)
            })
            .or_else(|| self.profiles_of(device).next())
    }

    pub fn active_profile_of_mut(&mut self, device: Uuid) -> Option<&mut Profile> {
        let id = self.active_profile_of(device)?.id;
        self.profiles.iter_mut().find(|p| p.id == id)
    }

    pub fn profile(&self, id: Uuid) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.id == id)
    }

    /// The dock a page is on, through its profile.
    pub fn device_of_page(&self, page: Uuid) -> Option<Uuid> {
        self.page_owner(page).map(|p| p.device)
    }

    /// Whether the profile is the one its dock shows.
    pub fn is_active(&self, profile: Uuid) -> bool {
        self.profile(profile)
            .and_then(|p| self.active_profile_of(p.device))
            .is_some_and(|active| active.id == profile)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_configuration_of_one_dock_becomes_the_first_dock() {
        let profile = Uuid::new_v4();
        let mut raw = json!({
            "schema": 2,
            "device": { "brightness": 40, "sleep_minutes": 5 },
            "active_profile": profile,
        });
        migrate(&mut raw);
        assert!(raw.get("device").is_none() && raw.get("active_profile").is_none());
        let devices: Vec<KnownDevice> = serde_json::from_value(raw["devices"].clone()).unwrap();
        assert_eq!(
            devices,
            [first_device(DeviceSettings {
                brightness: 40,
                sleep_minutes: 5,
            })]
        );
        assert_eq!(
            raw["active_profiles"][FIRST_DOCK.to_string()],
            json!(profile)
        );

        let before = raw.clone();
        migrate(&mut raw);
        assert_eq!(raw, before, "only once");
    }
}

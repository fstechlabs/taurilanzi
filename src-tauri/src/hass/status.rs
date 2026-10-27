//! What the app knows about Home Assistant: the connection, and the states, pictures,
//! histories and templates its keys show.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use serde::Serialize;

use super::entities::{Entities, Entity};
use crate::i18n::Language;
use crate::render::ImageData;

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, specta::Type)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum HassStatus {
    /// No key uses Home Assistant and nobody asked to connect.
    #[default]
    Off,
    Connecting,
    Connected {
        version: String,
        /// The name of the home, as set in Home Assistant.
        location: String,
    },
    /// Still trying every few seconds, except after a wrong token.
    Failed {
        reason: HassFailure,
        detail: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum HassFailure {
    /// No address yet, or one that is not a web address.
    NoAddress,
    /// Nothing answers there.
    Unreachable,
    /// Something answers, but it is not Home Assistant.
    NotHomeAssistant,
    /// The certificate is not one the system trusts.
    Certificate,
    TokenRequired,
    WrongToken,
    Other,
}

/// What the UI shows about Home Assistant. Never the token itself.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, specta::Type)]
pub struct HassInfo {
    pub status: HassStatus,
    pub token_set: bool,
}

/// How Home Assistant names an entity and shows its state, from its entity registry.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Registered {
    pub platform: String,
    pub translation_key: Option<String>,
    /// Decimals of a sensor, if the user or its integration set them.
    pub precision: Option<u8>,
}

/// One day or hour of a weather forecast.
#[derive(Debug, Clone, PartialEq)]
pub struct Forecast {
    /// Seconds since the epoch.
    pub at: f64,
    pub condition: String,
    pub temperature: Option<f64>,
    pub low: Option<f64>,
    pub precipitation_probability: Option<f64>,
}

/// A point of a history: when, in seconds since the epoch, and the value then.
pub type Point = (f64, f64);

/// Home Assistant's own settings that change how things read.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HomeConfig {
    pub temperature_unit: String,
    pub location: String,
    pub version: String,
}

/// Everything read from Home Assistant, each part shared so a report is cheap to copy. A part
/// that has not changed keeps its allocation, which is how two reports compare.
#[derive(Debug, Clone, Default)]
pub struct HassData {
    pub entities: Arc<Entities>,
    pub registry: Arc<HashMap<String, Registered>>,
    /// Home Assistant's own texts for states and attributes, in the app's language.
    pub labels: Arc<HashMap<String, String>>,
    pub config: Arc<HomeConfig>,
    pub history: Arc<BTreeMap<String, Arc<Vec<Point>>>>,
    /// What each template gives, or why it failed.
    pub templates: Arc<BTreeMap<String, Result<String, String>>>,
    pub forecasts: Arc<BTreeMap<String, Arc<Vec<Forecast>>>>,
    pub pictures: Arc<BTreeMap<String, ImageData>>,
}

impl PartialEq for HassData {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.entities, &other.entities)
            && Arc::ptr_eq(&self.registry, &other.registry)
            && Arc::ptr_eq(&self.labels, &other.labels)
            && Arc::ptr_eq(&self.config, &other.config)
            && Arc::ptr_eq(&self.history, &other.history)
            && Arc::ptr_eq(&self.templates, &other.templates)
            && Arc::ptr_eq(&self.forecasts, &other.forecasts)
            && Arc::ptr_eq(&self.pictures, &other.pictures)
    }
}

impl HassData {
    pub fn entity(&self, id: &str) -> Option<&Entity> {
        self.entities.get(id)
    }
}

/// What changed in a report, so only the keys showing it are drawn again.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Topic {
    Entity(String),
    Template(String),
    Forecast(String),
    Picture(String),
    History(String),
    /// Texts or names: everything reads differently.
    Everything,
}

/// Sent to the engine whenever any of it changes.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct HassReport {
    pub info: HassInfo,
    pub data: HassData,
    pub changed: BTreeSet<Topic>,
}

impl HassReport {
    pub fn connected(&self) -> bool {
        matches!(self.info.status, HassStatus::Connected { .. })
    }
}

/// What the engine needs from the client, from the configuration.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Wants {
    /// Some key of some profile uses Home Assistant.
    pub connect: bool,
    pub url: String,
    pub insecure: bool,
    pub language: Option<Language>,
    /// Entities keys act on, follow or show.
    pub entities: BTreeSet<String>,
    /// Entities shown as a graph, with the hours each one goes back.
    pub histories: BTreeMap<String, u32>,
    pub templates: BTreeSet<String>,
    pub forecasts: BTreeSet<String>,
    /// Entities shown as a picture, with how often a camera's is taken again, in seconds.
    pub pictures: BTreeMap<String, u32>,
}

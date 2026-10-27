//! What the editor offers to pick from: Home Assistant's entities, areas, devices and actions,
//! with its own texts and icons for them in the app's language.

use std::collections::{BTreeMap, HashMap};

use serde::Serialize;
use serde_json::{Value, json};

use super::entities::Entity;
use super::error::HassError;
use super::link::{Link, SLOW_REQUEST_TIMEOUT};
use super::status::Registered;

/// Texts of these categories are Home Assistant's names for states, attributes, actions and
/// domains, and the options of the fields of actions.
const TEXT_CATEGORIES: [&str; 5] = [
    "entity_component",
    "entity",
    "services",
    "title",
    "selector",
];
const ICON_CATEGORIES: [&str; 3] = ["entity_component", "entity", "services"];

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct HassCatalog {
    pub entities: Vec<HassEntityInfo>,
    pub areas: Vec<HassNamed>,
    pub floors: Vec<HassNamed>,
    pub labels: Vec<HassNamed>,
    pub devices: Vec<HassDevice>,
    /// Every action by domain, with its fields and their selectors, as `get_services` gives them.
    #[specta(type = BTreeMap<String, specta_typescript::Unknown>)]
    pub services: Value,
    /// Home Assistant's texts by key, such as `component.light.entity_component._.state.on`.
    pub texts: BTreeMap<String, String>,
    /// Home Assistant's icons by category, as `frontend/get_icons` gives them.
    #[specta(type = BTreeMap<String, specta_typescript::Unknown>)]
    pub icons: Value,
    /// The address of Home Assistant's web pages.
    pub url: String,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct HassEntityInfo {
    pub id: String,
    pub name: String,
    pub state: String,
    #[specta(type = BTreeMap<String, specta_typescript::Unknown>)]
    pub attributes: Value,
    pub area: Option<String>,
    pub device: Option<String>,
    pub platform: Option<String>,
    pub translation_key: Option<String>,
    /// One the user gave it, over the one of its domain.
    pub icon: Option<String>,
    /// Hidden in Home Assistant, or one of its settings or diagnostics.
    pub secondary: bool,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct HassNamed {
    pub id: String,
    pub name: String,
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct HassDevice {
    pub id: String,
    pub name: String,
    pub area: Option<String>,
}

/// One entity of the entity registry, in its short form for display.
#[derive(Debug, Clone, Default)]
struct Shown {
    platform: Option<String>,
    translation_key: Option<String>,
    area: Option<String>,
    device: Option<String>,
    icon: Option<String>,
    precision: Option<u8>,
    secondary: bool,
}

fn text(value: &Value) -> Option<String> {
    value.as_str().filter(|s| !s.is_empty()).map(str::to_string)
}

fn registry_display(link: &mut Link) -> Result<BTreeMap<String, Shown>, HassError> {
    let result = link.call_within(
        "config/entity_registry/list_for_display",
        Value::Null,
        SLOW_REQUEST_TIMEOUT,
    )?;
    let mut out = BTreeMap::new();
    for item in result["entities"].as_array().into_iter().flatten() {
        let Some(id) = item["ei"].as_str() else {
            continue;
        };
        out.insert(
            id.to_string(),
            Shown {
                platform: text(&item["pl"]),
                translation_key: text(&item["tk"]),
                area: text(&item["ai"]),
                device: text(&item["di"]),
                icon: text(&item["ic"]),
                precision: item["dp"].as_u64().map(|p| p.min(10) as u8),
                secondary: item["hb"].as_bool().unwrap_or(false) || !item["ec"].is_null(),
            },
        );
    }
    Ok(out)
}

/// How Home Assistant names and shows each entity, for drawing them.
pub fn registry(link: &mut Link) -> Result<HashMap<String, Registered>, HassError> {
    Ok(registry_display(link)?
        .into_iter()
        .map(|(id, shown)| {
            (
                id,
                Registered {
                    platform: shown.platform.unwrap_or_default(),
                    translation_key: shown.translation_key,
                    precision: shown.precision,
                },
            )
        })
        .collect())
}

/// Home Assistant's texts of `categories` in `language`, by key.
pub fn texts(
    link: &mut Link,
    language: &str,
    categories: &[&str],
) -> Result<BTreeMap<String, String>, HassError> {
    let mut out = BTreeMap::new();
    for category in categories {
        let result = link.call_within(
            "frontend/get_translations",
            json!({ "language": language, "category": category }),
            SLOW_REQUEST_TIMEOUT,
        )?;
        for (key, value) in result["resources"].as_object().into_iter().flatten() {
            if let Some(value) = value.as_str() {
                out.insert(key.clone(), value.to_string());
            }
        }
    }
    Ok(out)
}

/// The texts the keys need: what each state and option of an attribute is called.
pub fn state_texts(link: &mut Link, language: &str) -> Result<HashMap<String, String>, HassError> {
    Ok(texts(link, language, &["entity_component", "entity"])?
        .into_iter()
        .filter(|(key, _)| key.contains(".state."))
        .collect())
}

fn named(list: &Value, id: &str, name: &str) -> Vec<HassNamed> {
    let mut out: Vec<HassNamed> = list
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| {
            Some(HassNamed {
                id: text(&item[id])?,
                name: text(&item[name]).unwrap_or_default(),
                icon: text(&item["icon"]),
            })
        })
        .collect();
    out.sort_by_cached_key(|item| item.name.to_lowercase());
    out
}

pub fn read(link: &mut Link, language: &str, url: String) -> Result<HassCatalog, HassError> {
    let states = link.call_within("get_states", Value::Null, SLOW_REQUEST_TIMEOUT)?;
    let shown = registry_display(link)?;
    let devices_raw = link.call_within(
        "config/device_registry/list",
        Value::Null,
        SLOW_REQUEST_TIMEOUT,
    )?;
    let areas = named(
        &link.call("config/area_registry/list", Value::Null)?,
        "area_id",
        "name",
    );
    // Floors and labels came in later versions; without them the editor just offers none.
    let floors = link
        .call("config/floor_registry/list", Value::Null)
        .map(|list| named(&list, "floor_id", "name"))
        .unwrap_or_default();
    let labels = link
        .call("config/label_registry/list", Value::Null)
        .map(|list| named(&list, "label_id", "name"))
        .unwrap_or_default();
    let services = link.call_within("get_services", Value::Null, SLOW_REQUEST_TIMEOUT)?;
    let texts = texts(link, language, &TEXT_CATEGORIES)?;
    let mut icons = serde_json::Map::new();
    for category in ICON_CATEGORIES {
        let result = link
            .call("frontend/get_icons", json!({ "category": category }))
            .unwrap_or_default();
        icons.insert(category.into(), result["resources"].clone());
    }

    let mut devices: Vec<HassDevice> = devices_raw
        .as_array()
        .into_iter()
        .flatten()
        .filter(|device| device["disabled_by"].is_null())
        .filter_map(|device| {
            Some(HassDevice {
                id: text(&device["id"])?,
                name: text(&device["name_by_user"])
                    .or_else(|| text(&device["name"]))
                    .unwrap_or_default(),
                area: text(&device["area_id"]),
            })
        })
        .collect();
    devices.sort_by_cached_key(|item| item.name.to_lowercase());
    let device_area: HashMap<&str, &str> = devices
        .iter()
        .filter_map(|d| Some((d.id.as_str(), d.area.as_deref()?)))
        .collect();

    let mut entities: Vec<HassEntityInfo> = states
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|state| {
            let (id, entity) = Entity::from_full(state)?;
            let shown = shown.get(&id).cloned().unwrap_or_default();
            let area = shown.area.clone().or_else(|| {
                shown
                    .device
                    .as_deref()
                    .and_then(|d| device_area.get(d))
                    .map(|a| (*a).to_string())
            });
            Some(HassEntityInfo {
                name: entity.name().unwrap_or(&id).to_string(),
                state: entity.state.clone(),
                icon: shown
                    .icon
                    .or_else(|| entity.text("icon").map(str::to_string)),
                attributes: Value::Object(entity.attributes),
                area,
                device: shown.device,
                platform: shown.platform,
                translation_key: shown.translation_key,
                secondary: shown.secondary,
                id,
            })
        })
        .collect();
    entities.sort_by_cached_key(|item| item.name.to_lowercase());

    Ok(HassCatalog {
        entities,
        areas,
        floors,
        labels,
        devices,
        services,
        texts,
        icons: Value::Object(icons),
        url,
    })
}

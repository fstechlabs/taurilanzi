//! The state of entities as Home Assistant reports them, kept up to date by the compressed
//! messages of `subscribe_entities`.

use std::collections::{BTreeMap, BTreeSet};

use jiff::Timestamp;
use serde_json::{Map, Value};

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Entity {
    pub state: String,
    pub attributes: Map<String, Value>,
    /// Seconds since the epoch.
    pub last_changed: f64,
    pub last_updated: f64,
}

pub type Entities = BTreeMap<String, Entity>;

impl Entity {
    /// From `get_states`, which spells everything out.
    pub fn from_full(value: &Value) -> Option<(String, Self)> {
        let id = value["entity_id"].as_str()?.to_string();
        let time = |field: &str| {
            value[field]
                .as_str()
                .and_then(|text| text.parse::<Timestamp>().ok())
                .map_or(0.0, |t| t.as_millisecond() as f64 / 1000.0)
        };
        let entity = Self {
            state: value["state"].as_str().unwrap_or_default().to_string(),
            attributes: value["attributes"].as_object().cloned().unwrap_or_default(),
            last_changed: time("last_changed"),
            last_updated: time("last_updated"),
        };
        Some((id, entity))
    }

    pub fn attribute(&self, name: &str) -> Option<&Value> {
        self.attributes.get(name).filter(|value| !value.is_null())
    }

    pub fn number(&self, name: &str) -> Option<f64> {
        self.attribute(name).and_then(number)
    }

    pub fn text(&self, name: &str) -> Option<&str> {
        self.attribute(name).and_then(Value::as_str)
    }

    pub fn flag(&self, name: &str) -> Option<bool> {
        self.attribute(name).and_then(Value::as_bool)
    }

    /// A list of texts, such as `effect_list` or `options`.
    pub fn list(&self, name: &str) -> Vec<String> {
        self.attribute(name)
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| match item {
                        Value::String(text) => Some(text.clone()),
                        Value::Number(n) => Some(n.to_string()),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The state as a number, for sensors, numbers and counters.
    pub fn value(&self) -> Option<f64> {
        self.state.parse::<f64>().ok().filter(|v| v.is_finite())
    }

    pub fn available(&self) -> bool {
        !matches!(self.state.as_str(), "unavailable" | "unknown" | "")
    }

    pub fn name(&self) -> Option<&str> {
        self.text("friendly_name")
    }
}

pub fn number(value: &Value) -> Option<f64> {
    match value {
        Value::Number(n) => n.as_f64(),
        Value::String(text) => text.parse().ok(),
        _ => None,
    }
}

/// The domain of an entity id: `light` of `light.kitchen`.
pub fn domain(entity: &str) -> &str {
    entity.split('.').next().unwrap_or_default()
}

/// Applies one event of `subscribe_entities`: `a` adds whole entities, `c` changes some of their
/// fields (`+` sets, `-` removes attributes) and `r` removes them. Returns the ids that changed.
pub fn apply(entities: &mut Entities, event: &Value) -> BTreeSet<String> {
    let mut changed = BTreeSet::new();
    if let Some(added) = event["a"].as_object() {
        for (id, compressed) in added {
            let last_changed = compressed["lc"].as_f64().unwrap_or_default();
            let entity = Entity {
                state: compressed["s"].as_str().unwrap_or_default().to_string(),
                attributes: compressed["a"].as_object().cloned().unwrap_or_default(),
                last_changed,
                last_updated: compressed["lu"].as_f64().unwrap_or(last_changed),
            };
            if entities.get(id) != Some(&entity) {
                entities.insert(id.clone(), entity);
                changed.insert(id.clone());
            }
        }
    }
    if let Some(diffs) = event["c"].as_object() {
        for (id, diff) in diffs {
            let Some(entity) = entities.get_mut(id) else {
                continue;
            };
            let before = entity.clone();
            let plus = &diff["+"];
            if let Some(state) = plus["s"].as_str() {
                entity.state = state.to_string();
            }
            if let Some(attributes) = plus["a"].as_object() {
                for (name, value) in attributes {
                    entity.attributes.insert(name.clone(), value.clone());
                }
            }
            if let Some(at) = plus["lc"].as_f64() {
                entity.last_changed = at;
                entity.last_updated = at;
            }
            if let Some(at) = plus["lu"].as_f64() {
                entity.last_updated = at;
            }
            if let Some(removed) = diff["-"]["a"].as_array() {
                for name in removed.iter().filter_map(Value::as_str) {
                    entity.attributes.remove(name);
                }
            }
            if *entity != before {
                changed.insert(id.clone());
            }
        }
    }
    if let Some(removed) = event["r"].as_array() {
        for id in removed.iter().filter_map(Value::as_str) {
            if entities.remove(id).is_some() {
                changed.insert(id.to_string());
            }
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn adds_changes_and_removes_entities() {
        let mut entities = Entities::new();
        let added = apply(
            &mut entities,
            &json!({ "a": { "light.ceiling": {
                "s": "on", "a": { "brightness": 180, "friendly_name": "Techo" }, "c": "x", "lc": 10.5
            }}}),
        );
        assert_eq!(added, BTreeSet::from(["light.ceiling".to_string()]));
        let light = &entities["light.ceiling"];
        assert_eq!(light.state, "on");
        assert_eq!(light.number("brightness"), Some(180.0));
        assert_eq!((light.last_changed, light.last_updated), (10.5, 10.5));

        let changed = apply(
            &mut entities,
            &json!({ "c": { "light.ceiling": {
                "+": { "s": "off", "lc": 20.0, "a": { "brightness": null } },
                "-": { "a": ["friendly_name"] }
            }}}),
        );
        assert_eq!(changed.len(), 1);
        let light = &entities["light.ceiling"];
        assert_eq!(light.state, "off");
        assert_eq!(light.number("brightness"), None);
        assert_eq!(light.name(), None);
        assert_eq!(light.last_changed, 20.0);

        let same = apply(
            &mut entities,
            &json!({ "c": { "light.ceiling": { "+": { "s": "off" } } } }),
        );
        assert!(same.is_empty());
        let unknown = apply(
            &mut entities,
            &json!({ "c": { "light.other": { "+": { "s": "on" } } } }),
        );
        assert!(unknown.is_empty());

        let removed = apply(
            &mut entities,
            &json!({ "r": ["light.ceiling", "light.gone"] }),
        );
        assert_eq!(removed, BTreeSet::from(["light.ceiling".to_string()]));
        assert!(entities.is_empty());
    }

    #[test]
    fn reads_full_states() {
        let (id, entity) = Entity::from_full(&json!({
            "entity_id": "sensor.temp",
            "state": "21.5",
            "attributes": { "unit_of_measurement": "°C", "options": ["a", 2] },
            "last_changed": "2026-10-26T18:03:38.231206+00:00",
            "last_updated": "2026-10-26T18:03:38.231206+00:00",
        }))
        .unwrap();
        assert_eq!(id, "sensor.temp");
        assert_eq!(entity.value(), Some(21.5));
        assert_eq!(entity.text("unit_of_measurement"), Some("°C"));
        assert_eq!(entity.list("options"), ["a", "2"]);
        assert!(entity.last_changed > 1.7e9);
        assert!(entity.available());
        assert_eq!(domain("sensor.temp"), "sensor");
    }
}

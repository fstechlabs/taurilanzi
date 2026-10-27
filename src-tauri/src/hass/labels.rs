//! States and attributes in Home Assistant's own words: `on` of a door is "Abierta", `rainy` is
//! "Lluvioso". Its texts come in the app's language; anything without one shows as it is.

use super::entities::{Entity, domain};
use super::status::HassData;

/// The state of `id` as Home Assistant shows it.
pub fn state(data: &HassData, id: &str, entity: &Entity) -> String {
    match entity.state.as_str() {
        "unavailable" => return tr!("hass.states.unavailable").into(),
        "unknown" | "" => return tr!("hass.states.unknown").into(),
        _ => {}
    }
    lookup(data, id, entity, None, &entity.state).unwrap_or_else(|| entity.state.clone())
}

/// The value of an attribute, such as a climate's `hvac_action`, as Home Assistant shows it.
pub fn attribute(data: &HassData, id: &str, entity: &Entity, name: &str, value: &str) -> String {
    lookup(data, id, entity, Some(name), value).unwrap_or_else(|| value.to_string())
}

fn lookup(
    data: &HassData,
    id: &str,
    entity: &Entity,
    attribute: Option<&str>,
    value: &str,
) -> Option<String> {
    let domain = domain(id);
    let tail = match attribute {
        Some(name) => format!("state_attributes.{name}.state.{value}"),
        None => format!("state.{value}"),
    };
    let mut keys = Vec::with_capacity(3);
    if let Some(registered) = data.registry.get(id)
        && let Some(key) = &registered.translation_key
    {
        keys.push(format!(
            "component.{}.entity.{domain}.{key}.{tail}",
            registered.platform
        ));
    }
    if let Some(class) = entity.text("device_class") {
        keys.push(format!(
            "component.{domain}.entity_component.{class}.{tail}"
        ));
    }
    keys.push(format!("component.{domain}.entity_component._.{tail}"));
    keys.iter().find_map(|key| data.labels.get(key).cloned())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Arc;

    use serde_json::json;

    use super::*;
    use crate::hass::status::Registered;

    fn entity(state: &str, attributes: serde_json::Value) -> Entity {
        Entity {
            state: state.into(),
            attributes: attributes.as_object().cloned().unwrap_or_default(),
            ..Default::default()
        }
    }

    #[test]
    fn prefers_the_most_specific_text() {
        let labels: HashMap<String, String> = [
            (
                "component.binary_sensor.entity_component._.state.on",
                "Detectado",
            ),
            (
                "component.binary_sensor.entity_component.door.state.on",
                "Abierta",
            ),
            ("component.demo.entity.sensor.mode.state.eco", "Ahorro"),
            (
                "component.climate.entity_component._.state_attributes.hvac_action.state.heating",
                "Calentando",
            ),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
        let registry = HashMap::from([(
            "sensor.mode".to_string(),
            Registered {
                platform: "demo".into(),
                translation_key: Some("mode".into()),
                precision: None,
            },
        )]);
        let data = HassData {
            labels: Arc::new(labels),
            registry: Arc::new(registry),
            ..Default::default()
        };
        let door = entity("on", json!({ "device_class": "door" }));
        assert_eq!(state(&data, "binary_sensor.front", &door), "Abierta");
        let motion = entity("on", json!({}));
        assert_eq!(state(&data, "binary_sensor.hall", &motion), "Detectado");
        assert_eq!(
            state(&data, "sensor.mode", &entity("eco", json!({}))),
            "Ahorro"
        );
        assert_eq!(
            state(&data, "sensor.other", &entity("eco", json!({}))),
            "eco"
        );
        assert_eq!(
            state(&data, "light.a", &entity("unavailable", json!({}))),
            "No disponible"
        );
        let climate = entity("heat", json!({}));
        assert_eq!(
            attribute(&data, "climate.a", &climate, "hvac_action", "heating"),
            "Calentando"
        );
        assert_eq!(
            attribute(&data, "climate.a", &climate, "hvac_action", "idle"),
            "idle"
        );
    }
}

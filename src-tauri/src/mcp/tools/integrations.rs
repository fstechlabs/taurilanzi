//! The OBS Studios and Home Assistants keys talk to, and the names they have, to point keys at
//! real scenes, sources and entities. Their passwords and tokens stay out of reach: the user
//! types them in the app.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use super::context::{Answer, engine_error, json, refused, text};
use super::ids::parse_id;
use crate::config::Integration;
use crate::mcp::handler::Assistant;
use crate::obs::ObsList;

const DEFAULT_LIMIT: usize = 50;

#[derive(Deserialize, JsonSchema)]
pub struct SaveParams {
    /// An `Integration` (see get_schema): `{"kind": "obs", "name", "host", "port"}` or
    /// `{"kind": "home_assistant", "name", "url", "insecure"}`. With the `id` of one that exists
    /// it changes it; without, it adds one.
    pub integration: Value,
}

#[derive(Deserialize, JsonSchema)]
pub struct ServerParams {
    /// Id of the integration, from get_overview.
    pub server: String,
}

#[derive(Deserialize, JsonSchema)]
pub struct ObsNamesParams {
    pub server: String,
    /// What to list: `{"kind": "scenes"}`, `sources`, `audio_inputs`, `media_inputs`,
    /// `transitions`, `profiles`, `scene_collections`, `{"kind": "scene_items", "scene": "…"}`
    /// or `{"kind": "filters", "source": "…"}`.
    pub list: Value,
}

#[derive(Deserialize, JsonSchema)]
pub struct EntitiesParams {
    pub server: String,
    /// Words to find in the entity id, name, area or device.
    pub search: Option<String>,
    /// Only this domain, such as `light`, `switch` or `media_player`.
    pub domain: Option<String>,
    /// At most this many; 50 by default.
    pub limit: Option<usize>,
}

#[derive(Deserialize, JsonSchema)]
pub struct ServicesParams {
    pub server: String,
    /// The domain whose actions to describe, such as `light` or `script`.
    pub domain: String,
}

#[tool_router(router = integrations, vis = "pub(in crate::mcp)")]
impl Assistant {
    #[tool(
        description = "Adds an OBS Studio or a Home Assistant, or changes one. Passwords and tokens are not set here: tell the user to type them in the app (Settings → Integrations). Changing the address of one forgets its password or token, so it is never sent somewhere else."
    )]
    async fn save_integration(&self, Parameters(params): Parameters<SaveParams>) -> Answer {
        let mut value = params.integration;
        if !value.is_object() {
            return refused("integration is an object: see get_schema(\"Integration\")");
        }
        if value.get("id").is_none_or(Value::is_null) {
            value["id"] = json!(Uuid::new_v4());
        }
        let integration: Integration = match serde_json::from_value(value) {
            Ok(integration) => integration,
            Err(err) => return refused(format!("that is not an Integration: {err}")),
        };
        let before = match self.tools.engine.config().await {
            Ok(config) => config
                .integrations
                .into_iter()
                .find(|known| known.id() == integration.id()),
            Err(err) => return engine_error(err),
        };
        // Checked before forgetting anything, so a change the engine refuses loses nothing.
        if let Err(problem) = crate::config::rules::check_integration(&integration) {
            return refused(problem.to_string());
        }
        if before.as_ref().is_some_and(|before| {
            std::mem::discriminant(before) != std::mem::discriminant(&integration)
        }) {
            return refused("an integration keeps its kind: add another one instead");
        }
        let moved = before
            .as_ref()
            .is_some_and(|before| moved(before, &integration));
        let id = integration.id();
        // Before saving, so the client never takes the secret to the new address.
        if moved && let Some(before) = &before {
            self.forget_secret(before).await;
        }
        if let Err(err) = self
            .tools
            .engine
            .save_integration(integration.clone())
            .await
        {
            return engine_error(err);
        }
        self.tools.edited(false);
        let needs = match integration {
            Integration::Obs { .. } => "its password, if OBS asks for one",
            Integration::HomeAssistant { .. } => "a long-lived access token",
        };
        json(json!({
            "id": id,
            "note": format!("ask the user to type {needs} in the app: Settings → Integrations"),
        }))
    }

    #[tool(
        description = "Removes an OBS Studio or a Home Assistant, with its password or token. Keys that use it stay, and stop working until they point to another.",
        annotations(destructive_hint = true)
    )]
    async fn delete_integration(&self, Parameters(params): Parameters<ServerParams>) -> Answer {
        let id = match parse_id(&params.server, "integration") {
            Ok(id) => id,
            Err(err) => return refused(err),
        };
        match self.tools.engine.delete_integration(id).await {
            Ok(()) => {
                self.tools.edited(false);
                text("removed")
            }
            Err(err) => engine_error(err),
        }
    }

    #[tool(
        description = "Connects again to an OBS Studio or a Home Assistant now, without waiting for the next try."
    )]
    async fn reconnect_integration(&self, Parameters(params): Parameters<ServerParams>) -> Answer {
        let id = match parse_id(&params.server, "integration") {
            Ok(id) => id,
            Err(err) => return refused(err),
        };
        if let Some(obs) = self.tools.engine.obs(id) {
            obs.connect();
        } else if let Some(hass) = self.tools.engine.hass(id) {
            hass.connect();
        } else {
            return refused(format!("there is no integration {id}"));
        }
        text("connecting; get_overview shows how it went")
    }

    #[tool(
        description = "Names an OBS Studio has, to point OBS actions at them: scenes, sources, inputs, the items of a scene, the filters of a source, transitions, profiles or scene collections.",
        annotations(read_only_hint = true)
    )]
    async fn obs_names(&self, Parameters(params): Parameters<ObsNamesParams>) -> Answer {
        let id = match parse_id(&params.server, "integration") {
            Ok(id) => id,
            Err(err) => return refused(err),
        };
        let list: ObsList = match serde_json::from_value(params.list) {
            Ok(list) => list,
            Err(err) => return refused(format!("that is not a list OBS has: {err}")),
        };
        let Some(obs) = self.tools.engine.obs(id) else {
            return refused(format!("there is no OBS {id}"));
        };
        match obs.names(list).await {
            Ok(names) => json(names),
            Err(err) => refused(err.to_string()),
        }
    }

    #[tool(
        description = "Entities of a Home Assistant, to point Home Assistant actions and widgets at them: id, name, state, area, device and main attributes.",
        annotations(read_only_hint = true)
    )]
    async fn hass_entities(&self, Parameters(params): Parameters<EntitiesParams>) -> Answer {
        let id = match parse_id(&params.server, "integration") {
            Ok(id) => id,
            Err(err) => return refused(err),
        };
        let Some(hass) = self.tools.engine.hass(id) else {
            return refused(format!("there is no Home Assistant {id}"));
        };
        let catalog = match hass.catalog().await {
            Ok(catalog) => catalog,
            Err(err) => return refused(err.to_string()),
        };
        let words: Vec<String> = params
            .search
            .unwrap_or_default()
            .split_whitespace()
            .map(str::to_lowercase)
            .collect();
        let limit = params.limit.unwrap_or(DEFAULT_LIMIT);
        let mut total = 0;
        let entities: Vec<Value> = catalog
            .entities
            .iter()
            .filter(|entity| {
                params
                    .domain
                    .as_deref()
                    .is_none_or(|domain| entity.id.split('.').next() == Some(domain))
            })
            .filter(|entity| {
                let haystack = format!(
                    "{} {} {} {}",
                    entity.id,
                    entity.name,
                    entity.area.as_deref().unwrap_or_default(),
                    entity.device.as_deref().unwrap_or_default()
                )
                .to_lowercase();
                words.iter().all(|word| haystack.contains(word))
            })
            .inspect(|_| total += 1)
            .take(limit)
            .map(|entity| {
                json!({
                    "id": entity.id,
                    "name": entity.name,
                    "state": entity.state,
                    "area": entity.area,
                    "device": entity.device,
                    "unit": entity.attributes.get("unit_of_measurement"),
                    "device_class": entity.attributes.get("device_class"),
                    "secondary": entity.secondary,
                })
            })
            .collect();
        json(json!({ "entities": entities, "matching": total }))
    }

    #[tool(
        description = "The actions of a Home Assistant domain, with their fields, for `call` commands of Home Assistant actions.",
        annotations(read_only_hint = true)
    )]
    async fn hass_services(&self, Parameters(params): Parameters<ServicesParams>) -> Answer {
        let id = match parse_id(&params.server, "integration") {
            Ok(id) => id,
            Err(err) => return refused(err),
        };
        let Some(hass) = self.tools.engine.hass(id) else {
            return refused(format!("there is no Home Assistant {id}"));
        };
        match hass.catalog().await {
            Ok(catalog) => match catalog.services.get(&params.domain) {
                Some(services) => json(services),
                None => refused(format!("Home Assistant has no domain {}", params.domain)),
            },
            Err(err) => refused(err.to_string()),
        }
    }
}

impl Assistant {
    async fn forget_secret(&self, integration: &Integration) {
        let id = integration.id();
        let result = match integration {
            Integration::Obs { .. } => match self.tools.engine.obs(id) {
                Some(obs) => obs.set_password(None).await.map_err(|err| err.to_string()),
                None => Ok(()),
            },
            Integration::HomeAssistant { .. } => match self.tools.engine.hass(id) {
                Some(hass) => hass.set_token(None).await.map_err(|err| err.to_string()),
                None => Ok(()),
            },
        };
        if let Err(err) = result {
            log::warn!("could not forget the secret of a moved integration: {err}");
        }
    }
}

/// Whether an integration now points somewhere else, where its secret must not follow.
fn moved(before: &Integration, after: &Integration) -> bool {
    match (before, after) {
        (
            Integration::Obs { host, port, .. },
            Integration::Obs {
                host: new_host,
                port: new_port,
                ..
            },
        ) => host.trim() != new_host.trim() || port != new_port,
        (
            Integration::HomeAssistant { url, .. },
            Integration::HomeAssistant { url: new_url, .. },
        ) => url.trim().trim_end_matches('/') != new_url.trim().trim_end_matches('/'),
        _ => true,
    }
}

//! The servers the app talks to: OBS Studios and Home Assistants, each with a name of its own.
//! Passwords and tokens live in the secrets file, never here.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::actions::Action;
use super::model::Config;
use super::obs::ObsSettings;

const OBS_DEFAULT_HOST: &str = "127.0.0.1";
const OBS_DEFAULT_PORT: u16 = 4455;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Integration {
    Obs {
        id: Uuid,
        name: String,
        host: String,
        port: u16,
    },
    HomeAssistant {
        id: Uuid,
        name: String,
        /// As the browser opens it: `http://homeassistant.local:8123`.
        url: String,
        /// Accepts certificates that cannot be verified, as a Home Assistant with one of its own has.
        #[serde(default)]
        insecure: bool,
    },
}

impl Integration {
    pub fn id(&self) -> Uuid {
        match self {
            Self::Obs { id, .. } | Self::HomeAssistant { id, .. } => *id,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Self::Obs { name, .. } | Self::HomeAssistant { name, .. } => name,
        }
    }

    /// What is missing for it to be saved.
    pub fn problem(&self) -> Option<&'static str> {
        if self.name().trim().is_empty() {
            return Some("name the server");
        }
        match self {
            Self::HomeAssistant { url, .. } if url.trim().is_empty() => {
                Some("write the address of Home Assistant")
            }
            _ => None,
        }
    }
}

/// Where obs-websocket listens: an empty host or port means the default one.
pub fn obs_address(host: &str, port: u16) -> (String, u16) {
    let host = host.trim();
    let host = if host.is_empty() {
        OBS_DEFAULT_HOST
    } else {
        host
    };
    let port = if port == 0 { OBS_DEFAULT_PORT } else { port };
    (host.to_string(), port)
}

/// Before servers had names, the app talked to one OBS, set in the preferences. Keys that used it
/// keep it as a server whose id is nil, which is what their actions point to.
pub fn migrate(config: &mut Config, legacy_obs: Option<ObsSettings>) {
    let Some(settings) = legacy_obs else {
        return;
    };
    if config.integrations.iter().any(|i| i.id().is_nil()) {
        return;
    }
    let uses_obs = config
        .profiles
        .iter()
        .flat_map(|profile| &profile.pages)
        .flat_map(|page| page.keys.values())
        .flat_map(|key| key.all_actions())
        .any(uses_obs);
    if uses_obs {
        config.integrations.insert(
            0,
            Integration::Obs {
                id: Uuid::nil(),
                name: "OBS".into(),
                host: settings.host,
                port: settings.port,
            },
        );
    }
}

fn uses_obs(action: &Action) -> bool {
    match action {
        Action::Obs { .. } => true,
        Action::Random { actions } => actions.iter().any(uses_obs),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_address_means_the_default() {
        assert_eq!(obs_address(" ", 0), ("127.0.0.1".into(), 4455));
        assert_eq!(
            obs_address(" 192.168.1.5 ", 4456),
            ("192.168.1.5".into(), 4456)
        );
    }

    #[test]
    fn servers_need_a_name_and_home_assistant_an_address() {
        let obs = |name: &str| Integration::Obs {
            id: Uuid::new_v4(),
            name: name.into(),
            host: String::new(),
            port: 0,
        };
        assert_eq!(obs(" ").problem(), Some("name the server"));
        assert_eq!(obs("Streaming").problem(), None);
        let hass = Integration::HomeAssistant {
            id: Uuid::new_v4(),
            name: "Casa".into(),
            url: " ".into(),
            insecure: false,
        };
        assert_eq!(hass.problem(), Some("write the address of Home Assistant"));
    }

    fn stored(actions: &str) -> String {
        format!(
            r#"{{"schema":2,"revision":7,"device":{{"brightness":50,"sleep_minutes":0}},
            "preferences":{{"obs":{{"host":"192.168.1.5","port":4456}}}},
            "active_profile":"6f1c0d52-4a43-4f39-9a0e-1a2b3c4d5e6f",
            "profiles":[{{"id":"6f1c0d52-4a43-4f39-9a0e-1a2b3c4d5e6f","name":"P",
              "home_page":"0e1f2a3b-4c5d-4e6f-8a9b-0c1d2e3f4a5b",
              "pages":[{{"id":"0e1f2a3b-4c5d-4e6f-8a9b-0c1d2e3f4a5b","name":"Inicio","keys":{{
                "0":{{"appearance":{{"background":{{"type":"solid","color":{{"r":0,"g":0,"b":0}}}},"layers":[]}},
                     "actions":{actions},"double_press":[],"long_press":[],"second_state":null}}}}}}]}}]}}"#
        )
    }

    fn load(json: &str) -> Config {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("config.json"), json).unwrap();
        super::super::Store::open(dir.path().to_path_buf())
            .unwrap()
            .load()
            .unwrap()
            .unwrap()
    }

    #[test]
    fn the_one_obs_of_before_becomes_a_server_its_keys_point_to() {
        let config = load(&stored(
            r#"[{"type":"obs","command":{"kind":"record","mode":"toggle"}}]"#,
        ));
        assert_eq!(
            config.integrations,
            [Integration::Obs {
                id: Uuid::nil(),
                name: "OBS".into(),
                host: "192.168.1.5".into(),
                port: 4456,
            }]
        );
        let Action::Obs { server, .. } = &config.profiles[0].pages[0].keys[&0].actions[0] else {
            panic!("not an OBS action");
        };
        assert!(server.is_nil());
        let saved = serde_json::to_value(&config).unwrap();
        assert!(saved["preferences"].get("obs").is_none());
    }

    #[test]
    fn configurations_without_obs_keys_get_no_server() {
        let config = load(&stored(r#"[{"type":"back"}]"#));
        assert!(config.integrations.is_empty());
    }
}

//! The servers of the configuration: a client for each, started and stopped as they come and go.

use log::warn;
use uuid::Uuid;

use super::commands::{CommandError, Input};
use super::runtime::Engine;
use crate::config::{Action, Config, Integration};
use crate::hass::{Hass, code_account, token_account};
use crate::obs::{Obs, password_account};

impl Engine {
    /// Starts a client for each new server and lets go of those of servers that are gone.
    pub(super) fn sync_integrations(&mut self) {
        let ids: Vec<Uuid> = self
            .config
            .integrations
            .iter()
            .map(Integration::id)
            .collect();
        self.context.obs.retain(|id| ids.contains(&id));
        self.context.hass.retain(|id| ids.contains(&id));
        self.obs.retain(|id, _| ids.contains(id));
        self.hass.retain(|id, _| ids.contains(id));
        self.obs_wants.retain(|id, _| ids.contains(id));
        self.hass_wants.retain(|id, _| ids.contains(id));
        for integration in &self.config.integrations {
            let id = integration.id();
            match integration {
                Integration::Obs { .. } if !self.context.obs.contains(id) => {
                    let inputs = self.inputs.clone();
                    let client = Obs::start(
                        self.context.secrets.clone(),
                        password_account(id),
                        move |report| inputs.send(Input::Obs(id, report)).is_ok(),
                    );
                    self.context.obs.insert(id, client);
                }
                Integration::HomeAssistant { .. } if !self.context.hass.contains(id) => {
                    let (inputs, remote) = (self.inputs.clone(), self.inputs.clone());
                    let client = Hass::start(
                        self.context.secrets.clone(),
                        id,
                        move |report| inputs.send(Input::Hass(id, report)).is_ok(),
                        move |data| remote.send(Input::HassRemote(data)).is_ok(),
                    );
                    self.context.hass.insert(id, client);
                }
                _ => {}
            }
        }
        self.changed = true;
    }

    /// Adds a server, or changes the one with its id.
    pub(super) fn save_integration(
        &mut self,
        integration: Integration,
    ) -> Result<(), CommandError> {
        if let Some(problem) = integration.problem() {
            return Err(CommandError::InvalidActions(problem));
        }
        let id = integration.id();
        match self.config.integrations.iter_mut().find(|i| i.id() == id) {
            Some(existing) => {
                if std::mem::discriminant(existing) != std::mem::discriminant(&integration) {
                    return Err(CommandError::InvalidActions(
                        "a server cannot change its kind",
                    ));
                }
                *existing = integration;
            }
            None => self.config.integrations.push(integration),
        }
        self.save()?;
        self.sync_integrations();
        Ok(())
    }

    /// Removes a server and its password or token. Keys that used it say so in the editor.
    pub(super) fn delete_integration(&mut self, id: Uuid) -> Result<(), CommandError> {
        let Some(at) = self.config.integrations.iter().position(|i| i.id() == id) else {
            return Ok(());
        };
        let removed = self.config.integrations.remove(at);
        self.save()?;
        self.sync_integrations();
        let account = match removed {
            Integration::Obs { .. } => password_account(id),
            Integration::HomeAssistant { .. } => token_account(id),
        };
        if let Err(err) = self.context.secrets.set(&account, None) {
            warn!("could not delete the secret of a removed server: {err}");
        }
        Ok(())
    }
}

/// Where the secrets of the configuration are kept: passwords and tokens of its servers, and the
/// codes its keys use.
pub(super) fn secret_accounts(config: &Config) -> Vec<String> {
    let servers = config.integrations.iter().map(|i| match i {
        Integration::Obs { id, .. } => password_account(*id),
        Integration::HomeAssistant { id, .. } => token_account(*id),
    });
    let codes = config
        .profiles
        .iter()
        .flat_map(|p| &p.pages)
        .flat_map(|page| page.keys.values())
        .flat_map(|key| key.all_actions())
        .filter_map(|action| match action {
            Action::HomeAssistant { server, command } => {
                command.entity().map(|entity| code_account(*server, entity))
            }
            _ => None,
        });
    let mut accounts: Vec<String> = servers.chain(codes).collect();
    accounts.sort();
    accounts.dedup();
    accounts
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use uuid::Uuid;

    use super::*;
    use crate::config::{Appearance, DeviceSettings, HassCommand, Key, Page, Position, Profile};
    use crate::render::{Background, Color};

    #[test]
    fn lists_the_secrets_of_the_servers_and_of_the_keys_codes() {
        let (obs, hass) = (Uuid::from_u128(1), Uuid::from_u128(2));
        let key = Key {
            appearance: Appearance {
                background: Background::Solid {
                    color: Color::BLACK,
                },
                layers: Vec::new(),
            },
            actions: vec![Action::HomeAssistant {
                server: hass,
                command: HassCommand::Run {
                    entity: "lock.door".into(),
                },
            }],
            double_press: Vec::new(),
            long_press: Vec::new(),
            second_state: None,
        };
        let page = Page {
            id: Uuid::new_v4(),
            name: "Inicio".into(),
            keys: BTreeMap::from([(0, key)]),
            position: Position::default(),
        };
        let config = Config {
            schema: 2,
            revision: 0,
            device: DeviceSettings {
                brightness: 50,
                sleep_minutes: 0,
            },
            preferences: Default::default(),
            active_profile: Uuid::nil(),
            profiles: vec![Profile {
                id: Uuid::nil(),
                name: "P".into(),
                home_page: page.id,
                pages: vec![page],
                apps: Vec::new(),
            }],
            plugin_settings: Default::default(),
            integrations: vec![
                Integration::Obs {
                    id: obs,
                    name: "OBS".into(),
                    host: "127.0.0.1".into(),
                    port: 4455,
                },
                Integration::HomeAssistant {
                    id: hass,
                    name: "Casa".into(),
                    url: "http://casa.local:8123".into(),
                    insecure: false,
                },
            ],
            local_profiles: Default::default(),
            plugins: Default::default(),
            declined_plugins: Default::default(),
        };
        assert_eq!(
            secret_accounts(&config),
            [
                code_account(hass, "lock.door"),
                token_account(hass),
                password_account(obs),
            ]
        );
    }
}

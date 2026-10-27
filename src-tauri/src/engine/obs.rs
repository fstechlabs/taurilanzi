//! What each OBS client is told about the configuration, and what it reports.

use std::collections::BTreeMap;

use uuid::Uuid;

use super::follow::{self, Followed};
use super::runtime::Engine;
use crate::config::{Action, Integration, Key, obs_address};
use crate::obs::{ObsReport, Wants, Watch};

impl Engine {
    pub(super) fn on_obs(&mut self, server: Uuid, report: ObsReport) {
        if self.obs.get(&server).map(|r| &r.info) != Some(&report.info) {
            self.changed = true;
        }
        self.obs.insert(server, report);
        self.follow_servers();
    }

    pub(super) fn tell_obs(&mut self) {
        for (server, wants) in self.obs_wants() {
            if self.obs_wants.get(&server) == Some(&wants) {
                continue;
            }
            if let Some(client) = self.context.obs.get(server) {
                client.want(wants.clone());
            }
            self.obs_wants.insert(server, wants);
        }
    }

    /// Each server connects only while some key of some profile uses it.
    fn obs_wants(&self) -> BTreeMap<Uuid, Wants> {
        let mut all: BTreeMap<Uuid, Wants> = self
            .config
            .integrations
            .iter()
            .filter_map(|integration| match integration {
                Integration::Obs { id, host, port, .. } => {
                    let (host, port) = obs_address(host, *port);
                    Some((
                        *id,
                        Wants {
                            host,
                            port,
                            ..Default::default()
                        },
                    ))
                }
                Integration::HomeAssistant { .. } => None,
            })
            .collect();
        let keys = self
            .config
            .profiles
            .iter()
            .flat_map(|profile| &profile.pages)
            .flat_map(|page| page.keys.values());
        for key in keys {
            for server in key.all_actions().flat_map(obs_servers) {
                if let Some(wants) = all.get_mut(&server) {
                    wants.connect = true;
                }
            }
            if let Some((server, watch)) = followed(key).filter(|(_, w)| w.is_of_an_item())
                && let Some(wants) = all.get_mut(&server)
            {
                wants.watches.insert(watch);
            }
        }
        all
    }
}

/// The OBS state a key of two states follows, and on which server: that of its first command
/// with a state, if it is one of OBS's.
pub(super) fn followed(key: &Key) -> Option<(Uuid, Watch)> {
    match follow::followed(key)? {
        Followed::Obs(server, watch) => Some((server, watch)),
        Followed::Hass(..) => None,
    }
}

/// The servers an action sends commands to, inside random picks too.
fn obs_servers(action: &Action) -> Vec<Uuid> {
    match action {
        Action::Obs { server, .. } => vec![*server],
        Action::Random { actions } => actions.iter().flat_map(obs_servers).collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{Appearance, KeyState, ObsCommand, ObsSwitch};
    use crate::render::{Background, Color};

    fn obs(command: ObsCommand) -> Action {
        Action::Obs {
            server: Uuid::nil(),
            command,
        }
    }

    fn scene(name: &str) -> Action {
        obs(ObsCommand::Scene {
            scene: name.into(),
            preview: false,
        })
    }

    fn key(first: Vec<Action>, second: Option<Vec<Action>>) -> Key {
        let appearance = Appearance {
            background: Background::Solid {
                color: Color::BLACK,
            },
            layers: Vec::new(),
        };
        Key {
            appearance: appearance.clone(),
            actions: first,
            double_press: Vec::new(),
            long_press: Vec::new(),
            second_state: second.map(|actions| KeyState {
                appearance,
                actions,
            }),
        }
    }

    #[test]
    fn follows_the_first_command_with_a_state() {
        let record = obs(ObsCommand::Record {
            mode: ObsSwitch::Toggle,
        });
        let first = key(
            vec![Action::Back, obs(ObsCommand::SaveReplay), record.clone()],
            Some(vec![scene("B")]),
        );
        assert_eq!(followed(&first), Some((Uuid::nil(), Watch::Recording)));
        let from_second = key(
            vec![obs(ObsCommand::Transition)],
            Some(vec![scene("B"), record]),
        );
        assert_eq!(
            followed(&from_second),
            Some((Uuid::nil(), Watch::ProgramScene("B".into())))
        );
    }

    #[test]
    fn only_keys_of_two_states_follow() {
        assert_eq!(followed(&key(vec![scene("A")], None)), None);
        assert_eq!(
            followed(&key(
                vec![Action::Back],
                Some(vec![obs(ObsCommand::SaveReplay)])
            )),
            None
        );
        let mut gestures = key(Vec::new(), Some(Vec::new()));
        gestures.long_press = vec![scene("A")];
        assert_eq!(followed(&gestures), None);
    }

    #[test]
    fn commands_inside_random_actions_count_as_using_obs() {
        assert_eq!(obs_servers(&scene("A")), [Uuid::nil()]);
        assert_eq!(
            obs_servers(&Action::Random {
                actions: vec![Action::Back, scene("A")],
            }),
            [Uuid::nil()]
        );
        assert!(
            obs_servers(&Action::Random {
                actions: vec![Action::Back],
            })
            .is_empty()
        );
        assert!(obs_servers(&Action::Back).is_empty());
    }
}

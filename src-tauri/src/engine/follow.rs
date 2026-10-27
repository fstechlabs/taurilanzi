//! Keys of two states that follow a server, OBS or Home Assistant: the first command of the key
//! that has a state decides which server and what of it.

use uuid::Uuid;

use super::runtime::Engine;
use crate::config::{Action, Key};
use crate::hass::Watch as HassWatch;
use crate::obs::Watch as ObsWatch;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Followed {
    Obs(Uuid, ObsWatch),
    Hass(Uuid, HassWatch),
}

pub fn followed(key: &Key) -> Option<Followed> {
    let second = key.second_state.as_ref()?;
    key.actions
        .iter()
        .chain(&second.actions)
        .find_map(|action| match action {
            Action::Obs { server, command } => {
                ObsWatch::of(command).map(|watch| Followed::Obs(*server, watch))
            }
            Action::HomeAssistant { server, command } => {
                HassWatch::of(command).map(|watch| Followed::Hass(*server, watch))
            }
            _ => None,
        })
}

impl Engine {
    /// Keys of two states that follow a server show their second state while what they follow
    /// is on, and their first one while the server is not connected.
    pub(super) fn follow_servers(&mut self) {
        let Some(profile) = self.config.active_profile() else {
            return;
        };
        let mut moved = Vec::new();
        for page in &profile.pages {
            for (&index, key) in &page.keys {
                let on = match followed(key) {
                    Some(Followed::Obs(server, watch)) => self
                        .obs
                        .get(&server)
                        .is_some_and(|obs| obs.connected() && obs.states.is_on(&watch)),
                    Some(Followed::Hass(server, watch)) => self
                        .hass
                        .get(&server)
                        .is_some_and(|hass| hass.connected() && watch.is_on(&hass.data)),
                    None => continue,
                };
                if on != self.second_states.contains(&(page.id, index)) {
                    moved.push((page.id, index, on));
                }
            }
        }
        if moved.is_empty() {
            return;
        }
        let current = self.navigation.current(profile);
        self.changed = true;
        let mut redraw = Vec::new();
        for (page, index, on) in moved {
            if on {
                self.second_states.insert((page, index));
            } else {
                self.second_states.remove(&(page, index));
            }
            if page == current {
                redraw.push(index);
            }
        }
        self.redraw_indexes(redraw);
    }
}

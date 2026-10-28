//! A key's actions: checked before they are saved, run in order when it is pressed.

use std::thread;

use log::warn;
use uuid::Uuid;

use super::commands::{CommandError, Input, KeySlot};
use super::gestures::Gesture;
use super::runtime::Engine;
use crate::actions;
use crate::config::{Action, Config, Integration, Key, Profile};
use crate::plugins::{Instance, event, key_fields, key_name, settings_object};

const MAX_DELAY_MS: u32 = 60_000;

impl Engine {
    /// On a thread of their own, because opening apps or waiting takes time. Steps that move the
    /// dock come back to the engine as inputs, so they still happen in the order of the list.
    pub(super) fn run_actions(&self, list: Vec<Action>, slot: KeySlot) {
        if list.is_empty() {
            return;
        }
        let inputs = self.inputs.clone();
        let context = self.context.clone();
        let plugins = self.plugin_keys.host().cloned();
        thread::spawn(move || {
            for action in list {
                if let Action::Plugin {
                    action,
                    id,
                    settings,
                } = &action
                {
                    if let Some(host) = &plugins {
                        let instance = Instance {
                            action: action.clone(),
                            key: key_name(slot.index),
                            actionid: id.to_string(),
                        };
                        host.to_main(
                            &host.plugin_id(action),
                            &event("run", &instance, key_fields(&settings_object(settings))),
                        );
                    }
                    continue;
                }
                if action.navigates() {
                    if inputs.send(Input::Navigate(action)).is_err() {
                        break;
                    }
                    continue;
                }
                let result = actions::run(&action, &context);
                if let Err(err) = &result {
                    warn!("action failed: {err}");
                }
                if action.shows_result() || (result.is_err() && action.shows_failure()) {
                    let ok = result.is_ok();
                    if inputs.send(Input::KeyResult { slot, ok }).is_err() {
                        break;
                    }
                }
            }
        });
    }

    /// Does what a gesture on a key does, as if it happened on the dock, from any page.
    pub(super) fn press_key(
        &mut self,
        page: Uuid,
        index: u8,
        gesture: Gesture,
    ) -> Result<(), CommandError> {
        let profile = self.profile()?;
        let page_keys = &profile
            .page(page)
            .ok_or(CommandError::UnknownPage(page))?
            .keys;
        if !page_keys.contains_key(&index) {
            return Err(CommandError::InvalidKey(index));
        }
        self.perform(page, index, gesture);
        Ok(())
    }

    pub(super) fn navigate_to(&mut self, step: Action) {
        match step {
            Action::OpenPage { page, .. } => self.navigate(|nav, profile| nav.open(profile, page)),
            Action::Back => self.navigate(|nav, profile| nav.back(profile)),
            Action::Home => self.navigate(|nav, profile| nav.home(profile)),
            Action::PreviousPage => self.step_page(-1),
            Action::NextPage => self.step_page(1),
            Action::SwitchProfile { profile } => {
                if let Err(err) = self.set_profile(profile) {
                    warn!("could not switch profile: {err}");
                }
            }
            _ => {}
        }
    }

    /// In the order of the profile's pages, wrapping around, so one key can cycle through all.
    fn step_page(&mut self, by: isize) {
        self.navigate(|nav, profile| {
            let count = profile.pages.len() as isize;
            if count < 2 {
                return false;
            }
            let current = nav.current(profile);
            let at = profile
                .pages
                .iter()
                .position(|page| page.id == current)
                .unwrap_or(0) as isize;
            let next = profile.pages[(at + by).rem_euclid(count) as usize].id;
            nav.open(profile, next)
        });
    }
}

/// Each list of a key follows the rules of a list; the line on the canvas is only for a simple
/// press, and two states do not mix with gestures.
pub(super) fn check_key(
    config: &Config,
    profile: &Profile,
    page: Uuid,
    key: &Key,
) -> Result<(), CommandError> {
    check_actions(config, profile, page, &key.actions)?;
    let plugins = key
        .actions
        .iter()
        .filter(|a| matches!(a, Action::Plugin { .. }))
        .count();
    if plugins > 1 {
        return Err(CommandError::InvalidActions(
            "a key can hold one plugin action at most",
        ));
    }
    if plugins == 1 && key.second_state.is_some() {
        return Err(CommandError::InvalidActions(
            "a key of a plugin cannot have two states: the plugin draws it",
        ));
    }
    if key.second_state.is_some() && key.has_gestures() {
        return Err(CommandError::InvalidActions(
            "a key with two states cannot have gestures",
        ));
    }
    let others = [&key.double_press, &key.long_press]
        .into_iter()
        .chain(key.second_state.iter().map(|state| &state.actions));
    for list in others {
        if list.iter().any(|a| matches!(a, Action::OpenPage { .. })) {
            return Err(CommandError::InvalidActions(
                "only a simple press can lead to a page of the canvas",
            ));
        }
        if list.iter().any(|a| matches!(a, Action::Plugin { .. })) {
            return Err(CommandError::InvalidActions(
                "a plugin action only goes in the list of a simple press",
            ));
        }
        check_actions(config, profile, page, list)?;
    }
    Ok(())
}

/// A key may move the dock once, only to another page of its profile, and never from inside
/// a random pick.
pub(super) fn check_actions(
    config: &Config,
    profile: &Profile,
    page: Uuid,
    list: &[Action],
) -> Result<(), CommandError> {
    if list.iter().filter(|a| a.navigates()).count() > 1 {
        return Err(CommandError::InvalidActions(
            "a key can only move the dock once",
        ));
    }
    for action in list {
        check(config, profile, page, action)?;
    }
    Ok(())
}

fn check(
    config: &Config,
    profile: &Profile,
    page: Uuid,
    action: &Action,
) -> Result<(), CommandError> {
    let invalid = |reason| Err(CommandError::InvalidActions(reason));
    match action {
        Action::OpenPage { page: target, .. } => {
            if *target == page {
                return invalid("a key cannot lead to its own page");
            }
            if profile.page(*target).is_none() {
                return Err(CommandError::UnknownPage(*target));
            }
        }
        Action::SwitchProfile { profile: target } => {
            if !config.profiles.iter().any(|p| p.id == *target) {
                return Err(CommandError::UnknownProfile(*target));
            }
        }
        Action::Open { path } | Action::CloseApp { app: path } if path.trim().is_empty() => {
            return invalid("choose what to open or close");
        }
        Action::TypeText { text, .. } if text.is_empty() => return invalid("write the text"),
        Action::Hotkey { keys } if keys.key.is_empty() => return invalid("record the shortcut"),
        Action::Delay { ms } if *ms > MAX_DELAY_MS => {
            return invalid("a wait can be at most a minute");
        }
        Action::PlaySound { volume, .. } if *volume > 100 => {
            return invalid("the volume goes up to 100");
        }
        Action::Random { actions } => {
            if actions.is_empty() {
                return invalid("a random action needs actions to pick from");
            }
            if actions.iter().any(|a| {
                a.navigates() || matches!(a, Action::Random { .. } | Action::Plugin { .. })
            }) {
                return invalid("a random action cannot move the dock or hold another one");
            }
            for inner in actions {
                check(config, profile, page, inner)?;
            }
        }
        Action::Obs { server, command } => {
            if !has_server(config, *server, false) {
                return invalid("choose the OBS server");
            }
            if let Some(problem) = command.problem() {
                return invalid(problem);
            }
        }
        Action::Http { request } => {
            if let Some(problem) = request.problem() {
                return invalid(problem);
            }
        }
        Action::HomeAssistant { server, command } => {
            if !has_server(config, *server, true) {
                return invalid("choose the Home Assistant server");
            }
            if let Some(problem) = command.problem() {
                return invalid(problem);
            }
        }
        Action::Plugin { action, .. } if action.split('.').count() < 5 => {
            return invalid("that is not the id of a plugin action");
        }
        _ => {}
    }
    Ok(())
}

fn has_server(config: &Config, server: Uuid, hass: bool) -> bool {
    config.integrations.iter().any(|integration| {
        integration.id() == server
            && matches!(integration, Integration::HomeAssistant { .. }) == hass
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::config::{DeviceSettings, KeyCombo, Page, Position};

    fn config() -> Config {
        let pages: Vec<Page> = (0..2)
            .map(|i| Page {
                id: Uuid::new_v4(),
                name: format!("P{i}"),
                keys: BTreeMap::new(),
                position: Position::default(),
            })
            .collect();
        let profile = Profile {
            id: Uuid::new_v4(),
            name: "Test".into(),
            home_page: pages[0].id,
            pages,
            apps: Vec::new(),
        };
        Config {
            schema: 2,
            revision: 0,
            device: DeviceSettings {
                brightness: 50,
                sleep_minutes: 0,
            },
            preferences: Default::default(),
            active_profile: profile.id,
            profiles: vec![profile],
            plugin_settings: Default::default(),
            integrations: Vec::new(),
            local_profiles: Default::default(),
        }
    }

    fn open_page(page: Uuid) -> Action {
        Action::OpenPage {
            page,
            waypoints: Vec::new(),
        }
    }

    #[test]
    fn keys_move_the_dock_once_and_only_to_other_pages() {
        let config = config();
        let p = &config.profiles[0];
        let (home, other) = (p.pages[0].id, p.pages[1].id);
        let url = Action::OpenUrl {
            url: "https://example.com".into(),
        };
        let check = |list: &[Action]| check_actions(&config, p, home, list);
        assert!(check(&[url.clone(), open_page(other)]).is_ok());
        assert!(check(&[Action::Back, url]).is_ok());
        assert!(check(&[Action::Back, Action::NextPage]).is_err());
        assert!(check(&[open_page(home)]).is_err());
        assert!(matches!(
            check(&[open_page(Uuid::new_v4())]),
            Err(CommandError::UnknownPage(_))
        ));
        assert!(matches!(
            check(&[Action::SwitchProfile {
                profile: Uuid::new_v4()
            }]),
            Err(CommandError::UnknownProfile(_))
        ));
    }

    #[test]
    fn gestures_and_states_follow_their_own_rules() {
        use crate::config::{Appearance, KeyState};
        use crate::render::{Background, Color};

        let config = config();
        let p = &config.profiles[0];
        let (home, other) = (p.pages[0].id, p.pages[1].id);
        let look = || Appearance {
            background: Background::Solid {
                color: Color::BLACK,
            },
            layers: Vec::new(),
        };
        let key = |long_press: Vec<Action>, second_state: Option<KeyState>| Key {
            appearance: look(),
            actions: vec![open_page(other)],
            double_press: Vec::new(),
            long_press,
            second_state,
        };
        let check = |key: &Key| check_key(&config, p, home, key);
        assert!(check(&key(vec![Action::Home], None)).is_ok());
        assert!(
            check(&key(vec![open_page(other)], None)).is_err(),
            "lines only for a press"
        );
        let second = KeyState {
            appearance: look(),
            actions: vec![Action::Back],
        };
        assert!(check(&key(Vec::new(), Some(second.clone()))).is_ok());
        assert!(
            check(&key(vec![Action::Home], Some(second))).is_err(),
            "two states and gestures do not mix"
        );
    }

    #[test]
    fn random_picks_only_from_actions_that_stay_on_the_page() {
        let config = config();
        let p = &config.profiles[0];
        let home = p.pages[0].id;
        let hotkey = Action::Hotkey {
            keys: KeyCombo {
                key: "KeyC".into(),
                command: true,
                shift: false,
                option: false,
                control: false,
            },
        };
        let random = |actions| Action::Random { actions };
        let check = |list: &[Action]| check_actions(&config, p, home, list);
        assert!(check(&[random(vec![hotkey.clone(), Action::StopSounds])]).is_ok());
        assert!(check(&[random(Vec::new())]).is_err());
        assert!(check(&[random(vec![Action::Home])]).is_err());
        assert!(check(&[random(vec![random(vec![hotkey])])]).is_err());
        assert!(check(&[Action::Delay { ms: 61_000 }]).is_err());
    }

    #[test]
    fn a_plugin_action_goes_alone_in_a_simple_press() {
        use crate::config::{Appearance, KeyState};
        use crate::render::{Background, Color};

        let config = config();
        let p = &config.profiles[0];
        let home = p.pages[0].id;
        let look = || Appearance {
            background: Background::Solid {
                color: Color::BLACK,
            },
            layers: Vec::new(),
        };
        let plugin = || Action::Plugin {
            action: "com.a.b.c.act".into(),
            id: Uuid::new_v4(),
            settings: serde_json::Value::Null,
        };
        let key = |actions: Vec<Action>| Key {
            appearance: look(),
            actions,
            double_press: Vec::new(),
            long_press: Vec::new(),
            second_state: None,
        };
        let check = |key: &Key| check_key(&config, p, home, key);
        assert!(check(&key(vec![plugin()])).is_ok());
        assert!(check(&key(vec![Action::StopSounds, plugin(), Action::Back])).is_ok());
        assert!(
            check(&key(vec![plugin(), plugin()])).is_err(),
            "one per key"
        );
        let short = Action::Plugin {
            action: "com.a.b.c".into(),
            id: Uuid::new_v4(),
            settings: serde_json::Value::Null,
        };
        assert!(
            check(&key(vec![short])).is_err(),
            "the id of a plugin, not of an action"
        );
        assert!(
            check(&key(vec![Action::Random {
                actions: vec![plugin()]
            }]))
            .is_err()
        );

        let mut long = key(Vec::new());
        long.long_press = vec![plugin()];
        assert!(check(&long).is_err());
        let mut double = key(Vec::new());
        double.double_press = vec![plugin()];
        assert!(check(&double).is_err());
        let mut gesture = key(vec![plugin()]);
        gesture.long_press = vec![Action::StopSounds];
        assert!(
            check(&gesture).is_ok(),
            "other gestures can do other things"
        );

        let state = |actions| KeyState {
            appearance: look(),
            actions,
        };
        let mut two_states = key(vec![plugin()]);
        two_states.second_state = Some(state(Vec::new()));
        assert!(check(&two_states).is_err(), "the plugin draws the key");
        let mut in_second = key(Vec::new());
        in_second.second_state = Some(state(vec![plugin()]));
        assert!(check(&in_second).is_err());
    }
}

//! What each Home Assistant client is told about the configuration, what it reports, and the
//! keys that show it.

use std::collections::{BTreeMap, BTreeSet};

use log::warn;
use serde_json::Value;
use uuid::Uuid;

use super::runtime::Engine;
use crate::config::{
    Action, Appearance, DeviceSettings, Integration, LayerConfig, TileDrawing, Widget,
};
use crate::hass::{HassReport, Topic, Wants, domain};

/// How often a card of a camera or an image fetches its picture while it is on screen.
const CARD_PICTURE_SECONDS: u32 = 10;

impl Engine {
    pub(super) fn on_hass(&mut self, server: Uuid, report: HassReport) {
        let status_changed = self.hass.get(&server).map(|r| &r.info) != Some(&report.info);
        if status_changed {
            self.changed = true;
        }
        let changed = report.changed.clone();
        self.hass.insert(server, report);
        self.follow_servers();
        self.redraw_hass(server, &changed, status_changed);
    }

    /// What an automation of Home Assistant asks with the `taurilanzi` event: `profile` and
    /// `page` by name, `brightness` from 0 to 100, and `wake` to bring the keys back.
    pub(super) fn on_hass_remote(&mut self, data: &Value) {
        let named = |field: &str| {
            data[field]
                .as_str()
                .map(str::trim)
                .filter(|n| !n.is_empty())
        };
        if data["wake"].as_bool() == Some(true) {
            self.activity.touch();
            if let Some(change) = self.screen.wake_from_idle() {
                self.apply_screen(change);
            }
        }
        if let Some(name) = named("profile") {
            let found = self
                .config
                .profiles
                .iter()
                .find(|p| p.name.eq_ignore_ascii_case(name))
                .map(|p| p.id);
            match found {
                Some(id) => {
                    if let Err(err) = self.set_profile(id) {
                        warn!("Home Assistant could not switch profile: {err}");
                    }
                }
                None => warn!("Home Assistant asked for an unknown profile: {name}"),
            }
        }
        if let Some(name) = named("page") {
            let found = self.config.active_profile().and_then(|profile| {
                profile
                    .pages
                    .iter()
                    .find(|p| p.name.eq_ignore_ascii_case(name))
                    .map(|p| p.id)
            });
            match found {
                Some(page) => self.navigate(|nav, profile| nav.open(profile, page)),
                None => warn!("Home Assistant asked for an unknown page: {name}"),
            }
        }
        if let Some(brightness) = data["brightness"].as_f64() {
            let settings = DeviceSettings {
                brightness: brightness.clamp(0.0, 100.0).round() as u8,
                ..self.config.device.clone()
            };
            if let Err(err) = self.set_device(settings) {
                warn!("Home Assistant could not change the brightness: {err}");
            }
        }
    }

    /// Draws again the keys on the dock that show what changed on `server`.
    fn redraw_hass(&mut self, server: Uuid, changed: &BTreeSet<Topic>, everything: bool) {
        if self.paused || !self.screen.showing_keys() {
            return;
        }
        let Some(page) = self.current_page() else {
            return;
        };
        let everything = everything || changed.contains(&Topic::Everything);
        let redraw: Vec<u8> = page
            .keys
            .iter()
            .filter(|(index, key)| {
                appearance_topics(self.appearance_now(page.id, **index, key))
                    .iter()
                    .any(|(on, topic)| *on == server && (everything || changed.contains(topic)))
            })
            .map(|(index, _)| *index)
            .collect();
        self.redraw_indexes(redraw);
    }

    pub(super) fn redraw_indexes(&mut self, indexes: Vec<u8>) {
        if indexes.is_empty() || !self.screen.showing_keys() {
            return;
        }
        let mut images = BTreeMap::new();
        for index in indexes {
            let Some(design) = self.design_for(index) else {
                continue;
            };
            match self.draw(index, &design) {
                Ok(png) => {
                    images.insert(index, png);
                }
                Err(err) => warn!("key {index} could not be drawn: {err}"),
            }
        }
        if !images.is_empty() {
            self.dock.set_keys(images);
        }
    }

    pub(super) fn tell_hass(&mut self) {
        for (server, wants) in self.hass_wants() {
            if self.hass_wants.get(&server) == Some(&wants) {
                continue;
            }
            if let Some(client) = self.context.hass.get(server) {
                client.want(wants.clone());
            }
            self.hass_wants.insert(server, wants);
        }
    }

    /// Every server stays connected, so automations can move the dock. Cameras are only
    /// watched while the page with them is on the dock.
    fn hass_wants(&self) -> BTreeMap<Uuid, Wants> {
        let mut all: BTreeMap<Uuid, Wants> = self
            .config
            .integrations
            .iter()
            .filter_map(|integration| match integration {
                Integration::HomeAssistant {
                    id, url, insecure, ..
                } => Some((
                    *id,
                    Wants {
                        connect: !url.trim().is_empty(),
                        url: url.trim().to_string(),
                        insecure: *insecure,
                        language: Some(crate::i18n::current()),
                        ..Default::default()
                    },
                )),
                Integration::Obs { .. } => None,
            })
            .collect();
        let keys = self
            .config
            .profiles
            .iter()
            .flat_map(|profile| &profile.pages)
            .flat_map(|page| page.keys.values());
        for key in keys {
            for action in key.all_actions() {
                add_action(&mut all, action);
            }
            for widget in key.appearances().flat_map(widgets) {
                if let Some(wants) = widget.hass_server().and_then(|s| all.get_mut(&s)) {
                    add_widget(wants, widget);
                }
            }
        }
        if !self.paused
            && self.screen.showing_keys()
            && let Some(page) = self.current_page()
        {
            for (&index, key) in &page.keys {
                for widget in widgets(self.appearance_now(page.id, index, key)) {
                    let (server, entity, seconds) = match widget {
                        Widget::HassPicture {
                            server,
                            entity,
                            seconds,
                            ..
                        } => (server, entity, *seconds),
                        Widget::HassEntity { server, entity, .. }
                            if matches!(domain(entity), "camera" | "image") =>
                        {
                            (server, entity, CARD_PICTURE_SECONDS)
                        }
                        Widget::HassTile { server, entity, .. }
                            if matches!(
                                domain(entity),
                                "camera" | "image" | "media_player" | "person" | "device_tracker"
                            ) =>
                        {
                            (server, entity, CARD_PICTURE_SECONDS)
                        }
                        _ => continue,
                    };
                    if let Some(wants) = all.get_mut(server) {
                        let seconds = wants
                            .pictures
                            .get(entity)
                            .map_or(seconds, |s| (*s).min(seconds));
                        wants.pictures.insert(entity.clone(), seconds);
                    }
                }
            }
        }
        for wants in all.values_mut() {
            wants.entities.retain(|entity| !entity.trim().is_empty());
        }
        all
    }
}

fn widgets(appearance: &Appearance) -> impl Iterator<Item = &Widget> {
    appearance.layers.iter().filter_map(|layer| match layer {
        LayerConfig::Widget { widget } => Some(widget),
        _ => None,
    })
}

fn add_action(all: &mut BTreeMap<Uuid, Wants>, action: &Action) {
    match action {
        Action::HomeAssistant { server, command } => {
            let Some(wants) = all.get_mut(server) else {
                return;
            };
            if let crate::config::HassCommand::Call { target, .. } = command {
                wants.entities.extend(target.entities.iter().cloned());
            } else if let Some(entity) = command.entity() {
                wants.entities.insert(entity.to_string());
            }
        }
        Action::Random { actions } => {
            for action in actions {
                add_action(all, action);
            }
        }
        _ => {}
    }
}

fn add_widget(wants: &mut Wants, widget: &Widget) {
    match widget {
        Widget::HassEntity { entity, .. } => {
            wants.entities.insert(entity.clone());
        }
        Widget::HassGraph { entity, hours, .. } => {
            wants.entities.insert(entity.clone());
            add_history(wants, entity, *hours);
        }
        Widget::HassEntities { entities, .. } => {
            wants
                .entities
                .extend(entities.iter().map(|item| item.entity.clone()));
        }
        Widget::HassPicture { entity, .. } => {
            wants.entities.insert(entity.clone());
        }
        Widget::HassWeather {
            entity, forecast, ..
        } => {
            wants.entities.insert(entity.clone());
            if *forecast {
                wants.forecasts.insert(entity.clone());
            }
        }
        Widget::HassTemplate { template, .. } if !template.trim().is_empty() => {
            wants.templates.insert(template.clone());
        }
        Widget::HassTile {
            entity,
            extra,
            drawing,
            hours,
            ..
        } => {
            wants.entities.insert(entity.clone());
            wants.entities.extend(extra.iter().cloned());
            if *drawing == TileDrawing::Graph {
                add_history(wants, entity, *hours);
            }
            if domain(entity) == "weather" {
                wants.forecasts.insert(entity.clone());
            }
        }
        _ => {}
    }
}

fn add_history(wants: &mut Wants, entity: &str, hours: u32) {
    let hours = hours.clamp(1, 24 * 7);
    let longest = wants
        .histories
        .get(entity)
        .map_or(hours, |h| (*h).max(hours));
    wants.histories.insert(entity.to_string(), longest);
}

/// What of Home Assistant a look shows, and of which server, to draw it again when that changes.
fn appearance_topics(appearance: &Appearance) -> Vec<(Uuid, Topic)> {
    let mut all = Vec::new();
    for widget in widgets(appearance) {
        let Some(server) = widget.hass_server() else {
            continue;
        };
        let mut topics = Vec::new();
        match widget {
            Widget::HassEntity { entity, .. } => {
                topics.push(Topic::Entity(entity.clone()));
                if matches!(domain(entity), "camera" | "image") {
                    topics.push(Topic::Picture(entity.clone()));
                }
            }
            Widget::HassGraph { entity, .. } => {
                topics.push(Topic::Entity(entity.clone()));
                topics.push(Topic::History(entity.clone()));
            }
            Widget::HassEntities { entities, .. } => topics.extend(
                entities
                    .iter()
                    .map(|item| Topic::Entity(item.entity.clone())),
            ),
            Widget::HassPicture { entity, .. } => {
                topics.push(Topic::Entity(entity.clone()));
                topics.push(Topic::Picture(entity.clone()));
            }
            Widget::HassWeather { entity, .. } => {
                topics.push(Topic::Entity(entity.clone()));
                topics.push(Topic::Forecast(entity.clone()));
            }
            Widget::HassTemplate { template, .. } => {
                topics.push(Topic::Template(template.clone()));
            }
            Widget::HassTile {
                entity,
                extra,
                drawing,
                ..
            } => {
                topics.push(Topic::Entity(entity.clone()));
                topics.extend(extra.iter().map(|e| Topic::Entity(e.clone())));
                topics.push(Topic::Picture(entity.clone()));
                if *drawing == TileDrawing::Graph {
                    topics.push(Topic::History(entity.clone()));
                }
                if domain(entity) == "weather" {
                    topics.push(Topic::Forecast(entity.clone()));
                }
            }
            _ => {}
        }
        all.extend(topics.into_iter().map(|topic| (server, topic)));
    }
    all
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::render::{Background, Color};

    fn tile(drawing: &str) -> Widget {
        serde_json::from_value(json!({
            "kind": "hass_tile", "server": Uuid::from_u128(1), "entity": "sensor.temp",
            "extra": "sensor.humidity", "drawing": drawing, "hours": 6,
            "style": { "color": { "r": 255, "g": 255, "b": 255 }, "accent": { "r": 0, "g": 0, "b": 0 }, "font_family": "Inter" },
        }))
        .unwrap()
    }

    #[test]
    fn a_tile_follows_its_entity_and_the_other_one_it_shows() {
        let mut wants = Wants::default();
        add_widget(&mut wants, &tile("auto"));
        assert_eq!(
            wants.entities,
            BTreeSet::from(["sensor.humidity".to_string(), "sensor.temp".to_string()])
        );
        assert!(wants.histories.is_empty());
        add_widget(&mut wants, &tile("graph"));
        assert_eq!(wants.histories.get("sensor.temp"), Some(&6));

        let appearance = Appearance {
            background: Background::Solid {
                color: Color::BLACK,
            },
            layers: vec![LayerConfig::Widget {
                widget: tile("graph"),
            }],
        };
        let topics = appearance_topics(&appearance);
        let server = Uuid::from_u128(1);
        for topic in [
            Topic::Entity("sensor.temp".into()),
            Topic::Entity("sensor.humidity".into()),
            Topic::History("sensor.temp".into()),
        ] {
            assert!(topics.contains(&(server, topic.clone())), "{topic:?}");
        }
    }

    #[test]
    fn keys_that_only_show_an_entity_follow_it_too() {
        let mut all = BTreeMap::from([(Uuid::nil(), Wants::default())]);
        add_action(
            &mut all,
            &Action::HomeAssistant {
                server: Uuid::nil(),
                command: crate::config::HassCommand::Show {
                    entity: "sensor.temp".into(),
                },
            },
        );
        assert!(all[&Uuid::nil()].entities.contains("sensor.temp"));
    }
}

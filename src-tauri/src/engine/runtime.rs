use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::AtomicU64;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::time::{Duration, Instant};

use crate::driver::DeviceInfo;
use log::{info, warn};
use tokio::sync::broadcast;
use uuid::Uuid;

use super::actions::check_key;
use super::app_profiles::AppProfiles;
use super::appearance::{self, PageVars};
use super::commands::{Command, CommandError, Input, KeySlot, Reply};
use super::display::timing;
use super::follow::followed;
use super::gestures::{Gesture, Gestures, Reacts};
use super::handle::Activity;
use super::history::{History, Target};
use super::key_widgets::KeyWidgets;
use super::live::Live;
use super::plugins::{PluginKeys, plugin_action};
use super::results::Results;
use super::screen::Screen;
use super::snapshot::{EngineEvent, ProfileSummary, Snapshot};
use super::sync::Saved;
use super::widgets::Feeds;
use crate::actions;
use crate::config::{
    AppRef, Appearance, Config, DeviceSettings, Key, LayerConfig, Navigation, Page, Profile,
    ScreensaverImage, Store, Widget, assets_in,
};
use crate::driver::d200::{D200, Event, KEY_COUNT, KEY_SIZE, WIDE_KEY, WIDE_KEY_SIZE};
use crate::hass::{HassReport, Wants as HassWants};
use crate::monitor::Monitor;
use crate::obs::{ObsReport, Wants};
use crate::render::{Color, KeyDesign, Layer, Renderer};
use resvg::tiny_skia::Pixmap;

/// Unused images are kept this long, in case an editor still has them open.
const ORPHAN_MIN_AGE: Duration = Duration::from_secs(24 * 60 * 60);
const PRUNE_INTERVAL: Duration = Duration::from_secs(60 * 60);

pub struct Engine {
    pub(super) store: Store,
    pub(super) config: Config,
    pub(super) navigation: Navigation,
    pub(super) renderer: Renderer,
    pub(super) dock: D200,
    events: broadcast::Sender<EngineEvent>,
    device: Option<DeviceInfo>,
    pub(super) paused: bool,
    pub(super) activity: Activity,
    /// Lets actions running on their own thread ask the engine to move the dock.
    pub(super) inputs: Sender<Input>,
    pub(super) context: actions::Context,
    pub(super) history: History,
    pub(super) gestures: Gestures,
    /// Keys of two states now in their second one. Runtime state, like the path.
    pub(super) second_states: HashSet<(Uuid, u8)>,
    /// Something the snapshot shows changed while handling the current input.
    pub(super) changed: bool,
    last_prune: Option<Instant>,
    pub(super) screen: Screen,
    /// The screensaver picture last drawn, and for which choice.
    pub(super) screensaver_drawn: Option<(ScreensaverImage, Pixmap)>,
    /// Bumped for every copy of the screensaver sent to the dock, so a slow, older one is dropped.
    pub(super) screensaver_uploads: Arc<AtomicU64>,
    /// Which profile to show as apps come to the front. Runtime state, like the path.
    pub(super) app_profiles: AppProfiles,
    pub(super) front_app: Option<AppRef>,
    pub(super) live: Live,
    key_widgets: KeyWidgets,
    /// Readings for the system monitor and network widgets, taken only while one is drawn.
    pub(super) monitor: Monitor,
    pub(super) results: Results,
    pub(super) plugin_keys: PluginKeys,
    /// Each OBS server as its client last reported it.
    pub(super) obs: BTreeMap<Uuid, ObsReport>,
    /// What each OBS client was last asked for, so it is only told about changes.
    pub(super) obs_wants: BTreeMap<Uuid, Wants>,
    /// Each Home Assistant server as its client last reported it.
    pub(super) hass: BTreeMap<Uuid, HassReport>,
    pub(super) hass_wants: BTreeMap<Uuid, HassWants>,
    /// When each profile, and the settings, were last edited since the app started.
    pub(super) edited: BTreeMap<Uuid, jiff::Timestamp>,
    pub(super) saved: Saved,
}

/// How the engine talks to the rest of the app.
pub struct Links {
    pub events: broadcast::Sender<EngineEvent>,
    pub activity: Activity,
    pub inputs: Sender<Input>,
    pub plugins: PluginKeys,
}

impl Engine {
    pub fn new(
        store: Store,
        config: Config,
        renderer: Renderer,
        dock: D200,
        links: Links,
        context: actions::Context,
    ) -> Self {
        let Links {
            events,
            activity,
            inputs,
            plugins,
        } = links;
        let navigation = store.load_navigation();
        let key_widgets = KeyWidgets::with_counters(store.load_counters());
        let app_profiles = AppProfiles::new(config.active_profile);
        let mut config = config;
        let exists: HashSet<Uuid> = config.profiles.iter().map(|p| p.id).collect();
        config.local_profiles.retain(|id| exists.contains(id));
        let saved = Saved::of(&config);
        Self {
            store,
            config,
            navigation,
            renderer,
            dock,
            events,
            device: None,
            paused: false,
            activity,
            inputs,
            context,
            history: History::default(),
            gestures: Gestures::default(),
            second_states: HashSet::new(),
            changed: false,
            last_prune: None,
            screen: Screen::default(),
            screensaver_drawn: None,
            screensaver_uploads: Arc::new(AtomicU64::new(0)),
            app_profiles,
            front_app: None,
            live: Live::default(),
            key_widgets,
            monitor: Monitor::default(),
            results: Results::default(),
            plugin_keys: plugins,
            obs: BTreeMap::new(),
            obs_wants: BTreeMap::new(),
            hass: BTreeMap::new(),
            hass_wants: BTreeMap::new(),
            edited: BTreeMap::new(),
            saved,
        }
    }

    /// Pushes the current page to the dock and serves dock events and commands until every
    /// sender is gone.
    pub fn run(mut self, inputs: Receiver<Input>) {
        self.gestures.set_timing(timing(&self.config.preferences));
        self.dock.set_brightness(self.config.device.brightness);
        self.dock
            .set_sleep_minutes(self.config.device.sleep_minutes);
        self.sync_integrations();
        self.show_current_page();
        self.store_screensaver();
        self.prune_assets();
        self.update_sampling();
        self.publish();

        loop {
            // Waits for the next input, or until a held or pending press, or a change of the
            // screen, is due.
            let due = self
                .gestures
                .deadline()
                .into_iter()
                .chain(self.screen_deadline())
                .chain(self.live_deadline())
                .chain(self.result_deadline())
                .min();
            let input = match due {
                Some(due) => {
                    match inputs.recv_timeout(due.saturating_duration_since(Instant::now())) {
                        Ok(input) => Some(input),
                        Err(RecvTimeoutError::Timeout) => None,
                        Err(RecvTimeoutError::Disconnected) => break,
                    }
                }
                None => match inputs.recv() {
                    Ok(input) => Some(input),
                    Err(_) => break,
                },
            };
            match input {
                Some(Input::Dock(event)) => self.on_dock(event),
                Some(Input::Command(command)) => self.on_command(command),
                Some(Input::Navigate(step)) => self.navigate_to(step),
                Some(Input::Locked(locked)) => self.on_lock(locked),
                Some(Input::FrontApp(app)) => self.on_front_app(app),
                Some(Input::Obs(server, report)) => self.on_obs(server, report),
                Some(Input::Hass(server, report)) => self.on_hass(server, report),
                Some(Input::HassRemote(data)) => self.on_hass_remote(&data),
                Some(Input::KeyResult { slot, ok }) => self.on_key_result(slot, ok),
                Some(Input::Plugin(input)) => self.on_plugin(input),
                None => {}
            }
            let now = Instant::now();
            let due = self.gestures.tick(now);
            self.fire(due);
            self.tick_screen(now);
            self.tick_live();
            self.tick_results(now);
            if self.changed {
                self.publish();
            }
        }
    }

    fn on_dock(&mut self, event: Event) {
        match event {
            Event::Connected(device) => {
                info!(
                    "dock ready: {} with firmware {}",
                    device.model, device.firmware
                );
                self.device = Some(device);
                self.changed = true;
            }
            Event::Disconnected => {
                info!("dock lost, waiting for it to come back");
                self.device = None;
                self.changed = true;
            }
            Event::Key { index, pressed } => {
                self.activity.touch();
                if self.paused {
                    return;
                }
                if !self.screen.showing_keys() {
                    if pressed {
                        self.press_while_away();
                    }
                    return;
                }
                self.emit(EngineEvent::Key { index, pressed });
                self.on_key(index, pressed);
            }
        }
    }

    fn on_command(&mut self, command: Command) {
        // Someone working on the keys wants to see them on the dock.
        if command.changes_keys() {
            self.activity.touch();
            if let Some(change) = self.screen.wake_from_idle() {
                self.apply_screen(change);
            }
        }
        match command {
            Command::Snapshot(reply) => {
                let _ = reply.send(self.snapshot());
            }
            Command::SetKey {
                page,
                index,
                key,
                reply,
            } => {
                let result =
                    self.edit(Target::Key { page, index }, |e| e.set_key(page, index, key));
                self.respond(reply, result);
            }
            Command::SetDevice { settings, reply } => {
                let result = self.set_device(settings);
                self.respond(reply, result);
            }
            Command::SetPreferences { preferences, reply } => {
                let result = self.set_preferences(preferences);
                self.respond(reply, result);
            }
            Command::TryScreensaver { on, reply } => {
                self.try_screensaver(on);
                self.respond(reply, ());
            }
            Command::KeyImage { page, index, reply } => {
                let _ = reply.send(self.key_image(page, index));
            }
            Command::Preview {
                page,
                index,
                appearance,
                reply,
            } => {
                let page = page.unwrap_or_else(|| self.current_page_id());
                let vars = self.page_vars(page);
                let mut design = appearance::resolve(
                    &appearance,
                    &self.feeds(),
                    &vars,
                    self.key_widgets.key_now((page, index), Instant::now()),
                );
                if let Some(key) = self
                    .profile()
                    .ok()
                    .and_then(|p| p.page(page))
                    .and_then(|p| p.keys.get(&index))
                {
                    self.with_plugin(key, &mut design);
                }
                let _ = reply.send(check_index(index).and_then(|()| self.draw(index, &design)));
            }
            Command::Fonts(reply) => {
                let _ = reply.send(self.renderer.font_families());
            }
            Command::SetPaused { paused, reply } => {
                self.set_paused(paused);
                self.respond(reply, ());
            }
            Command::SetProfile { id, reply } => {
                let result = self.set_profile(id);
                self.respond(reply, result);
            }
            Command::CreatePage {
                name,
                position,
                reply,
            } => {
                let result = self.edit(Target::Other, |e| e.create_page(name, position));
                self.respond(reply, result);
            }
            Command::UpdatePage { id, changes, reply } => {
                let result = self.edit(Target::Page(id), |e| e.update_page(id, changes));
                self.respond(reply, result);
            }
            Command::DuplicatePage { id, reply } => {
                let result = self.edit(Target::Other, |e| e.duplicate_page(id));
                self.respond(reply, result);
            }
            Command::DeletePage { id, reply } => {
                let result = self.edit(Target::Other, |e| e.delete_page(id));
                self.respond(reply, result);
            }
            Command::SetHomePage { id, reply } => {
                let result = self.edit(Target::Other, |e| e.set_home_page(id));
                self.respond(reply, result);
            }
            Command::ShowPage { id, reply } => {
                let result = self.show_page(id);
                self.respond(reply, result);
            }
            Command::MoveKey { from, to, reply } => {
                let result = self.edit(Target::Other, |e| e.move_key(from, to));
                self.respond(reply, result);
            }
            Command::PressKey {
                page,
                index,
                gesture,
                reply,
            } => {
                let result = self.press_key(page, index, gesture);
                self.respond(reply, result);
            }
            Command::Undo(reply) => {
                let result = self.undo();
                self.respond(reply, result);
            }
            Command::Redo(reply) => {
                let result = self.redo();
                self.respond(reply, result);
            }
            Command::CreateProfile { name, reply } => {
                let result = self.edit(Target::Other, |e| e.create_profile(name));
                self.respond(reply, result);
            }
            Command::RenameProfile { id, name, reply } => {
                let result = self.edit(Target::Other, |e| e.rename_profile(id, name));
                self.respond(reply, result);
            }
            Command::DuplicateProfile { id, reply } => {
                let result = self.edit(Target::Other, |e| e.duplicate_profile(id));
                self.respond(reply, result);
            }
            Command::DeleteProfile { id, reply } => {
                let result = self.edit(Target::Other, |e| e.delete_profile(id));
                self.respond(reply, result);
            }
            Command::MoveProfile { id, index, reply } => {
                let result = self.edit(Target::Other, |e| e.move_profile(id, index));
                self.respond(reply, result);
            }
            Command::LinkApp {
                profile,
                app,
                reply,
            } => {
                let result = self.edit(Target::Other, |e| e.link_app(profile, app));
                self.respond(reply, result);
            }
            Command::UnlinkApp {
                profile,
                app,
                reply,
            } => {
                let result = self.edit(Target::Other, |e| e.unlink_app(profile, app));
                self.respond(reply, result);
            }
            Command::SaveIntegration { integration, reply } => {
                let result = self.save_integration(integration);
                self.respond(reply, result);
            }
            Command::DeleteIntegration { id, reply } => {
                let result = self.delete_integration(id);
                self.respond(reply, result);
            }
            Command::SyncExport(reply) => {
                let _ = reply.send(self.sync_export());
            }
            Command::ApplySync { changes, reply } => {
                let result = self.apply_sync(changes);
                self.respond(reply, result);
            }
            Command::SetProfileLocal { id, local, reply } => {
                let result = self.set_profile_local(id, local);
                self.respond(reply, result);
            }
        }
    }

    fn snapshot(&self) -> Snapshot {
        let profile = self.config.active_profile();
        let path = profile.map(|p| self.navigation.path(p)).unwrap_or_default();
        Snapshot {
            connected: self.device.is_some(),
            device: self.device.clone(),
            revision: self.config.revision,
            settings: self.config.device.clone(),
            preferences: self.config.preferences.clone(),
            screen: self.screen.mode(),
            paused: self.paused,
            profiles: self
                .config
                .profiles
                .iter()
                .map(|p| ProfileSummary {
                    id: p.id,
                    name: p.name.clone(),
                    apps: p.apps.clone(),
                    local: self.config.local_profiles.contains(&p.id),
                })
                .collect(),
            active_profile: self.config.active_profile,
            home_page: profile.map(|p| p.home_page).unwrap_or_default(),
            pages: profile.map(|p| p.pages.clone()).unwrap_or_default(),
            path,
            second_states: self
                .second_states
                .iter()
                .map(|&(page, index)| KeySlot { page, index })
                .collect(),
            can_undo: self.history.can_undo(),
            can_redo: self.history.can_redo(),
            front_app: self.front_app.clone(),
            automatic_profile: self.automatic_profile(),
            integrations: self.config.integrations.clone(),
            obs: self
                .obs
                .iter()
                .map(|(id, r)| (*id, r.info.clone()))
                .collect(),
            hass: self
                .hass
                .iter()
                .map(|(id, r)| (*id, r.info.clone()))
                .collect(),
            language: crate::i18n::current(),
        }
    }

    fn set_key(&mut self, page: Uuid, index: u8, key: Option<Key>) -> Result<(), CommandError> {
        check_index(index)?;
        let mut key = key;
        if let Some(key) = &mut key {
            check_key(&self.config, self.profile()?, page, key)?;
            self.keep_plugin_settings(key);
            let here: Vec<Uuid> = self
                .profile()?
                .page(page)
                .and_then(|p| p.keys.get(&index))
                .map(|k| plugin_action(k).map(|(_, id, _)| id).into_iter().collect())
                .unwrap_or_default();
            let elsewhere: Vec<Uuid> = self
                .used_plugin_ids()
                .into_iter()
                .filter(|id| !here.contains(id))
                .collect();
            if plugin_action(key).is_some_and(|(_, id, _)| elsewhere.contains(&id)) {
                Self::renew_plugin_ids(std::iter::once(&mut *key));
            }
        }
        let target = self
            .profile_mut()?
            .page_mut(page)
            .ok_or(CommandError::UnknownPage(page))?;
        match key {
            Some(key) => target.keys.insert(index, key),
            None => target.keys.remove(&index),
        };
        self.save()?;

        if self.screen.showing_keys() && self.current_page().is_some_and(|p| p.id == page) {
            let design = self
                .design_for(index)
                .unwrap_or_else(|| KeyDesign::solid(Color::BLACK));
            let png = self.draw(index, &design)?;
            self.dock.set_keys(BTreeMap::from([(index, png)]));
        }
        Ok(())
    }

    pub(super) fn set_device(&mut self, settings: DeviceSettings) -> Result<(), CommandError> {
        let settings = DeviceSettings {
            brightness: settings.brightness.min(100),
            ..settings
        };
        if !self.paused && self.screen.showing_keys() {
            self.dock.set_brightness(settings.brightness);
        }
        self.dock.set_sleep_minutes(settings.sleep_minutes);
        self.config.device = settings;
        self.save()
    }

    /// Pausing darkens the screen instead of clearing it, so resuming is instant.
    fn set_paused(&mut self, paused: bool) {
        if paused == self.paused {
            return;
        }
        self.paused = paused;
        info!("dock paused: {paused}");
        if paused && let Some(change) = self.screen.reset() {
            self.apply_screen(change);
        }
        self.dock.set_brightness(if paused {
            0
        } else {
            self.config.device.brightness
        });
        self.changed = true;
    }

    /// Each profile keeps its own path, so coming back to one shows where it was left.
    pub(super) fn set_profile(&mut self, id: Uuid) -> Result<(), CommandError> {
        if !self.config.profiles.iter().any(|p| p.id == id) {
            return Err(CommandError::UnknownProfile(id));
        }
        self.chose_profile(id);
        if self.config.active_profile != id {
            self.config.active_profile = id;
            self.save()?;
            self.show_current_page();
        }
        Ok(())
    }

    fn key_image(&mut self, page: Uuid, index: u8) -> Result<Vec<u8>, CommandError> {
        check_index(index)?;
        let design = self
            .profile()?
            .page(page)
            .ok_or(CommandError::UnknownPage(page))?
            .keys
            .get(&index)
            .map(|key| self.design_of(page, index, key))
            .unwrap_or_else(|| KeyDesign::solid(Color::BLACK));
        self.draw(index, &design)
    }

    pub(super) fn save(&mut self) -> Result<(), CommandError> {
        self.store.save(&mut self.config)?;
        self.note_edits();
        self.changed = true;
        self.prune_assets();
        self.update_sampling();
        Ok(())
    }

    fn prune_assets(&mut self) {
        if self
            .last_prune
            .is_some_and(|at| at.elapsed() < PRUNE_INTERVAL)
        {
            return;
        }
        self.last_prune = Some(Instant::now());
        match self
            .store
            .assets
            .prune(&self.assets_in_use(), ORPHAN_MIN_AGE)
        {
            Ok(0) => {}
            Ok(removed) => info!("{removed} unused images deleted"),
            Err(err) => warn!("could not delete unused images: {err}"),
        }
    }

    /// Images and sounds of the configuration and of every version undo can bring back.
    fn assets_in_use(&self) -> HashSet<&str> {
        let mut used = assets_in(&self.config.profiles);
        for version in self.history.versions() {
            used.extend(assets_in(version));
        }
        if let ScreensaverImage::Custom { asset, source, .. } =
            &self.config.preferences.screensaver.image
        {
            used.insert(asset);
            used.extend(source.as_deref());
        }
        used
    }

    pub(super) fn profile(&self) -> Result<&Profile, CommandError> {
        self.config
            .active_profile()
            .ok_or(CommandError::NoActiveProfile)
    }

    pub(super) fn profile_mut(&mut self) -> Result<&mut Profile, CommandError> {
        self.config
            .active_profile_mut()
            .ok_or(CommandError::NoActiveProfile)
    }

    pub(super) fn current_page(&self) -> Option<&Page> {
        let profile = self.config.active_profile()?;
        profile.page(self.navigation.current(profile))
    }

    /// Moves the dock along the path of the active profile and shows the result.
    pub(super) fn navigate(&mut self, step: impl FnOnce(&mut Navigation, &Profile) -> bool) {
        let Some(profile) = self.config.active_profile() else {
            return;
        };
        if step(&mut self.navigation, profile) {
            self.save_navigation();
            self.show_current_page();
        }
    }

    pub(super) fn save_navigation(&self) {
        if let Err(err) = self.store.save_navigation(&self.navigation) {
            warn!("could not save where the dock is: {err}");
        }
    }

    pub(super) fn show_current_page(&mut self) {
        self.gestures.cancel();
        self.changed = true;
        // The screensaver owns the screen; the page goes up when it ends.
        if !self.screen.showing_keys() {
            return;
        }
        let mut images = BTreeMap::new();
        for index in (0..KEY_COUNT).chain([WIDE_KEY]) {
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
        self.dock.replace_page(images);
    }

    fn on_key(&mut self, index: u8, pressed: bool) {
        self.plugin_key(index, pressed);
        let reacts = self
            .key(index)
            .filter(|key| key.second_state.is_none())
            .map(|key| Reacts {
                double: !key.double_press.is_empty(),
                long: !key.long_press.is_empty()
                    || key_widget(&key.appearance).is_some_and(starts_over),
            })
            .unwrap_or_default();
        let now = Instant::now();
        let fired = if pressed {
            self.gestures.down(index, reacts, now)
        } else {
            self.gestures.up(index, now)
        };
        self.fire(fired);
    }

    fn fire(&mut self, fired: Vec<(u8, Gesture)>) {
        let page = self.current_page_id();
        for (index, gesture) in fired {
            self.perform(page, index, gesture);
        }
    }

    /// Does what a gesture on a key does, and moves a key of two states to its other one.
    pub(super) fn perform(&mut self, page: Uuid, index: u8, gesture: Gesture) {
        let Some(key) = self
            .profile()
            .ok()
            .and_then(|profile| profile.page(page))
            .and_then(|p| p.keys.get(&index))
        else {
            return;
        };
        let second = key
            .second_state
            .as_ref()
            .filter(|_| self.second_states.contains(&(page, index)));
        let list = match gesture {
            Gesture::Press => second.map_or(&key.actions, |state| &state.actions),
            Gesture::DoublePress => &key.double_press,
            Gesture::LongPress => &key.long_press,
        }
        .clone();
        let toggles =
            gesture == Gesture::Press && key.second_state.is_some() && followed(key).is_none();
        let widget = key_widget(self.appearance_now(page, index, key)).cloned();
        self.run_actions(list, KeySlot { page, index });
        if let Some(widget) = widget {
            self.press_widget(page, index, &widget, gesture);
        }
        if toggles {
            if !self.second_states.remove(&(page, index)) {
                self.second_states.insert((page, index));
            }
            self.changed = true;
            if self.screen.showing_keys()
                && self.current_page_id() == page
                && let Some(design) = self.design_for(index)
            {
                match self.draw(index, &design) {
                    Ok(png) => self.dock.set_keys(BTreeMap::from([(index, png)])),
                    Err(err) => warn!("key {index} could not be drawn: {err}"),
                }
            }
        }
    }

    fn key(&self, index: u8) -> Option<&Key> {
        self.current_page()?.keys.get(&index)
    }

    /// What a press does to the widget of its key: timers, stopwatches and counters answer to it.
    fn press_widget(&mut self, page: Uuid, index: u8, widget: &Widget, gesture: Gesture) {
        let slot = (page, index);
        let now = Instant::now();
        let widgets = &mut self.key_widgets;
        let changed = match (widget, gesture) {
            (Widget::Timer { seconds, .. }, Gesture::Press) => {
                widgets.press(slot, Duration::from_secs(u64::from((*seconds).max(1))), now);
                true
            }
            (Widget::Stopwatch { .. }, Gesture::Press) => {
                widgets.press_stopwatch(slot, now);
                true
            }
            (Widget::Counter { step, .. }, Gesture::Press) => {
                let changed = widgets.count(slot, *step);
                self.save_counters(changed);
                changed
            }
            (Widget::Phrases { .. }, Gesture::Press) => {
                widgets.press_phrases(slot);
                true
            }
            (Widget::Timer { .. }, Gesture::LongPress) => {
                widgets.reset(slot);
                true
            }
            (Widget::Stopwatch { .. }, Gesture::LongPress) => {
                widgets.reset_stopwatch(slot);
                true
            }
            (Widget::Counter { .. }, Gesture::LongPress) => {
                let changed = widgets.reset_counter(slot);
                self.save_counters(changed);
                changed
            }
            _ => false,
        };
        if changed {
            self.emit(EngineEvent::KeyImage { page, index });
            self.redraw_on_dock(page, index);
        }
    }

    fn save_counters(&self, changed: bool) {
        if changed && let Err(err) = self.store.save_counters(self.key_widgets.counters()) {
            warn!("could not save the counters: {err}");
        }
    }

    /// Draws a key again if it is on the dock now.
    fn redraw_on_dock(&mut self, page: Uuid, index: u8) {
        if !self.screen.showing_keys() || self.current_page_id() != page {
            return;
        }
        if let Some(design) = self.design_for(index) {
            match self.draw(index, &design) {
                Ok(png) => self.dock.set_keys(BTreeMap::from([(index, png)])),
                Err(err) => warn!("key {index} could not be drawn: {err}"),
            }
        }
    }

    pub(super) fn design_for(&self, index: u8) -> Option<KeyDesign> {
        let key = self.key(index)?;
        Some(self.design_of(self.current_page_id(), index, key))
    }

    /// How a key of `page` looks now: in its current state, with the page's values in its text.
    fn design_of(&self, page: Uuid, index: u8, key: &Key) -> KeyDesign {
        let appearance = key
            .second_state
            .as_ref()
            .filter(|_| self.second_states.contains(&(page, index)))
            .map_or(&key.appearance, |state| &state.appearance);
        let mut design = appearance::resolve(
            appearance,
            &self.feeds(),
            &self.page_vars(page),
            self.key_widgets.key_now((page, index), Instant::now()),
        );
        self.with_plugin(key, &mut design);
        if let Some(ok) = self.result_of(page, index) {
            design.layers.push(Layer::Status { ok });
        }
        design
    }

    /// A plugin key shows the plugin's image instead of the user's, and its title on top.
    fn with_plugin(&self, key: &Key, design: &mut KeyDesign) {
        let Some((image, title)) = self.plugin_layers(key) else {
            return;
        };
        if let Some(image) = image {
            design
                .layers
                .retain(|layer| !matches!(layer, Layer::Image { .. }));
            design.layers.insert(0, image);
        }
        design.layers.extend(title);
    }

    fn feeds(&self) -> Feeds<'_> {
        Feeds {
            monitor: &self.monitor,
            hass: &self.hass,
            assets: &self.store.assets,
        }
    }

    fn page_vars(&self, page: Uuid) -> PageVars {
        let Some(profile) = self.config.active_profile() else {
            return PageVars::default();
        };
        let at = profile.pages.iter().position(|p| p.id == page);
        PageVars {
            name: at
                .map(|i| profile.pages[i].name.clone())
                .unwrap_or_default(),
            number: at.map_or(0, |i| i + 1),
            count: profile.pages.len(),
        }
    }

    fn current_page_id(&self) -> Uuid {
        self.current_page().map(|page| page.id).unwrap_or_default()
    }

    /// Drops the second states of keys that no longer have two.
    pub(super) fn forget_stale_states(&mut self) {
        let Some(profile) = self.config.active_profile() else {
            return;
        };
        let keep: HashSet<(Uuid, u8)> = self
            .second_states
            .iter()
            .copied()
            .filter(|(page, index)| {
                profile
                    .page(*page)
                    .and_then(|p| p.keys.get(index))
                    .is_some_and(|key| key.second_state.is_some())
            })
            .collect();
        self.second_states = keep;
    }

    pub(super) fn draw(&mut self, index: u8, design: &KeyDesign) -> Result<Vec<u8>, CommandError> {
        let size = if index == WIDE_KEY {
            WIDE_KEY_SIZE
        } else {
            KEY_SIZE
        };
        self.renderer
            .render(design, size)
            .map(|png| png.as_ref().clone())
            .map_err(|err| CommandError::Render(err.to_string()))
    }

    /// Sends the new state before the reply, so whoever asked sees the change once it is done.
    fn respond<T>(&mut self, reply: Reply<T>, value: T) {
        if self.changed {
            self.publish();
        }
        let _ = reply.send(value);
    }

    fn publish(&mut self) {
        self.tell_obs();
        self.tell_hass();
        self.follow_servers();
        self.sync_plugins();
        self.changed = false;
        self.emit(EngineEvent::State(Arc::new(self.snapshot())));
    }

    pub(super) fn emit(&self, event: EngineEvent) {
        let _ = self.events.send(event);
    }
}

pub(super) fn check_index(index: u8) -> Result<(), CommandError> {
    if index < KEY_COUNT || index == WIDE_KEY {
        Ok(())
    } else {
        Err(CommandError::InvalidKey(index))
    }
}

/// The live content of a look, if it has some.
fn key_widget(appearance: &Appearance) -> Option<&Widget> {
    appearance.layers.iter().find_map(|layer| match layer {
        LayerConfig::Widget { widget } => Some(widget),
        _ => None,
    })
}

/// Widgets that start over when their key is held down.
fn starts_over(widget: &Widget) -> bool {
    matches!(
        widget,
        Widget::Timer { .. } | Widget::Stopwatch { .. } | Widget::Counter { .. }
    )
}

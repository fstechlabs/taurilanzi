//! What the dock screen shows when it is not the keys: the screensaver, or nothing. The rules
//! live in `screen`; this applies them to the dock.

use crate::i18n;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::{Duration, Instant};

use log::{info, warn};
use resvg::tiny_skia::Pixmap;

use uuid::Uuid;

use super::commands::CommandError;
use super::gestures::Timing;
use super::runtime::Engine;
use super::screen::{Change, Press, Rules};
use crate::config::{Preferences, ScreensaverImage};
use crate::driver::DeviceModel;
use crate::screensaver::{self, Source};

pub(super) fn timing(preferences: &Preferences) -> Timing {
    let gestures = &preferences.gestures;
    Timing {
        long_press: Duration::from_millis(u64::from(gestures.long_press_ms)),
        double_press: Duration::from_millis(u64::from(gestures.double_press_ms)),
    }
}

impl Engine {
    fn rules(&self) -> Rules {
        Rules::from(&self.config.preferences.screensaver)
    }

    /// A paused dock is dark already: nothing about the screen changes until it resumes.
    pub(super) fn screen_deadline(&self) -> Option<Instant> {
        if self.paused {
            return None;
        }
        let (rules, idle, now) = (self.rules(), self.activity.idle(), Instant::now());
        self.docks
            .sessions
            .values()
            .filter_map(|session| session.screen.deadline(&rules, idle, now))
            .min()
    }

    pub(super) fn tick_screen(&mut self, now: Instant) {
        if self.paused {
            return;
        }
        let (rules, idle) = (self.rules(), self.activity.idle());
        for device in self.docks.ids() {
            let change = self
                .docks
                .get_mut(device)
                .and_then(|session| session.screen.tick(&rules, idle, now));
            if let Some(change) = change {
                self.apply_screen(device, change);
            }
        }
    }

    pub(super) fn on_lock(&mut self, locked: bool) {
        let rules = self.rules();
        if !locked {
            // Back at the computer: the wait for the idle screensaver starts again.
            self.activity.touch();
        }
        for device in self.docks.ids() {
            let paused = self.paused;
            let Some(session) = self.docks.get_mut(device) else {
                continue;
            };
            if paused {
                session.screen.set_locked(locked);
                continue;
            }
            let change = if locked {
                session.screen.lock(&rules, Instant::now())
            } else {
                session.screen.unlock()
            };
            if let Some(change) = change {
                self.apply_screen(device, change);
            }
        }
    }

    /// A key pressed while the screensaver or the dark is up never does what the key does.
    pub(super) fn press_while_away(&mut self, device: Uuid) {
        let rules = self.rules();
        let press = self
            .docks
            .get_mut(device)
            .map(|session| session.screen.press(&rules, Instant::now()));
        if let Some(Press::Wake(change)) = press {
            self.apply_screen(device, change);
        }
    }

    /// Back to the keys on every dock left idle, as when someone edits them.
    pub(super) fn wake_from_idle(&mut self) {
        for device in self.docks.ids() {
            let change = self
                .docks
                .get_mut(device)
                .and_then(|session| session.screen.wake_from_idle());
            if let Some(change) = change {
                self.apply_screen(device, change);
            }
        }
    }

    pub(super) fn apply_screen(&mut self, device: Uuid, change: Change) {
        let saver = &self.config.preferences.screensaver;
        let brightness = match change {
            Change::Keys => self.settings_of(device).brightness,
            Change::Screensaver => saver.brightness,
            Change::Dark => 0,
        };
        let Some(session) = self.docks.get(device) else {
            return;
        };
        session
            .dock
            .set_brightness(if self.paused { 0 } else { brightness });
        let model = session.model;
        match change {
            Change::Keys => self.show_current_page(device),
            Change::Screensaver => match self.screensaver_panel(model) {
                Some(panel) => {
                    info!("screensaver on the dock");
                    if let Some(session) = self.docks.get(device) {
                        session.dock.replace_page(screensaver::keys(model, &panel));
                    }
                }
                None => warn!("could not draw the screensaver image"),
            },
            Change::Dark => info!("dock screen dark"),
        }
        self.changed = true;
    }

    fn screensaver_source(&self) -> Option<Source> {
        match &self.config.preferences.screensaver.image {
            ScreensaverImage::Scene { scene } => Some(Source::Scene(*scene)),
            ScreensaverImage::Custom { asset, .. } => self
                .store
                .assets
                .get(asset)
                .inspect_err(|err| warn!("screensaver image {asset}: {err}"))
                .ok()
                .map(Source::Image),
        }
    }

    /// The screensaver picture as the settings choose it, drawn once per choice and model.
    fn screensaver_panel(&mut self, model: &'static DeviceModel) -> Option<Pixmap> {
        let image = &self.config.preferences.screensaver.image;
        if let Some((drawn_for, drawn_model, panel)) = &self.screensaver_drawn
            && drawn_for == image
            && *drawn_model == model.id
        {
            return Some(panel.clone());
        }
        let panel = screensaver::draw(&self.screensaver_source()?, model.panel, 1.0)?;
        self.screensaver_drawn = Some((image.clone(), model.id, panel.clone()));
        Some(panel)
    }

    /// Gives a dock that keeps one its own copy of the screensaver, which it shows while the app
    /// is away. Drawn on its own thread: a large picture takes a while and the keys must not wait.
    pub(super) fn store_screensaver(&self, device: Uuid) {
        let Some(session) = self.docks.get(device) else {
            return;
        };
        if !session.model.screensaver {
            return;
        }
        let Some(source) = self.screensaver_source() else {
            return;
        };
        let (dock, panel) = (session.dock.clone(), session.model.panel);
        let uploads = Arc::clone(&self.screensaver_uploads);
        let mine = uploads.fetch_add(1, Ordering::SeqCst) + 1;
        let spawned = thread::Builder::new()
            .name("screensaver".into())
            .spawn(move || {
                let png = screensaver::draw(&source, panel, 1.0)
                    .and_then(|panel| panel.encode_png().ok());
                match png {
                    Some(png) if uploads.load(Ordering::SeqCst) == mine => {
                        dock.set_screensaver(png)
                    }
                    Some(_) => {}
                    None => warn!("could not draw the screensaver image"),
                }
            });
        if let Err(err) = spawned {
            warn!("could not draw the screensaver image: {err}");
        }
    }

    /// Not an edit of the keys, so undo leaves it alone, like the brightness.
    pub(super) fn set_preferences(&mut self, preferences: Preferences) -> Result<(), CommandError> {
        let before = self.config.preferences.screensaver.clone();
        let language = self.config.preferences.language;
        self.config.preferences = preferences.clamped();
        if self.config.preferences.language != language {
            i18n::set(i18n::resolve(self.config.preferences.language));
            // Widgets and default texts on the keys are drawn in the new language.
            self.show_all_pages();
        }
        let timing = timing(&self.config.preferences);
        for session in self.docks.sessions.values_mut() {
            session.gestures.set_timing(timing);
        }
        self.save()?;
        let after = &self.config.preferences.screensaver;
        let new_image = after.image != before.image;
        let new_brightness = after.brightness != before.brightness;
        let rules = self.rules();
        for device in self.docks.ids() {
            if new_image {
                self.store_screensaver(device);
            }
            let Some(session) = self.docks.get_mut(device) else {
                continue;
            };
            match session.screen.reconsider(&rules, Instant::now()) {
                Some(change) => self.apply_screen(device, change),
                // A screensaver on the dock shows a new image or brightness at once.
                None if session.screen.showing_screensaver() && (new_image || new_brightness) => {
                    self.apply_screen(device, Change::Screensaver);
                }
                None => {}
            }
        }
        Ok(())
    }

    /// Shows the screensaver on every dock for a while, or stops showing it.
    pub(super) fn try_screensaver(&mut self, on: bool) {
        for device in self.docks.ids() {
            let change = self.docks.get_mut(device).and_then(|session| {
                if on {
                    session.screen.try_it(Instant::now())
                } else {
                    session.screen.stop_trial()
                }
            });
            if let Some(change) = change {
                self.apply_screen(device, change);
            }
        }
    }
}

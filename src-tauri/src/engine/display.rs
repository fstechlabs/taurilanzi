//! What the dock screen shows when it is not the keys: the screensaver, or nothing. The rules
//! live in `screen`; this applies them to the dock.

use crate::i18n;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::{Duration, Instant};

use log::{info, warn};
use resvg::tiny_skia::Pixmap;

use super::commands::CommandError;
use super::gestures::Timing;
use super::runtime::Engine;
use super::screen::{Change, Press, Rules};
use crate::config::{Preferences, ScreensaverImage};
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
        self.screen
            .deadline(&self.rules(), self.activity.idle(), Instant::now())
    }

    pub(super) fn tick_screen(&mut self, now: Instant) {
        if self.paused {
            return;
        }
        if let Some(change) = self.screen.tick(&self.rules(), self.activity.idle(), now) {
            self.apply_screen(change);
        }
    }

    pub(super) fn on_lock(&mut self, locked: bool) {
        if self.paused {
            self.screen.set_locked(locked);
            return;
        }
        let change = if locked {
            self.screen.lock(&self.rules(), Instant::now())
        } else {
            // Back at the Mac: the wait for the idle screensaver starts again.
            self.activity.touch();
            self.screen.unlock()
        };
        if let Some(change) = change {
            self.apply_screen(change);
        }
    }

    /// A key pressed while the screensaver or the dark is up never does what the key does.
    pub(super) fn press_while_away(&mut self) {
        match self.screen.press(&self.rules(), Instant::now()) {
            Press::Wake(change) => self.apply_screen(change),
            Press::Act | Press::Ignore => {}
        }
    }

    pub(super) fn apply_screen(&mut self, change: Change) {
        let saver = &self.config.preferences.screensaver;
        let brightness = match change {
            Change::Keys => self.config.device.brightness,
            Change::Screensaver => saver.brightness,
            Change::Dark => 0,
        };
        self.dock
            .set_brightness(if self.paused { 0 } else { brightness });
        match change {
            Change::Keys => self.show_current_page(),
            Change::Screensaver => match self.screensaver_panel() {
                Some(panel) => {
                    info!("screensaver on the dock");
                    self.dock.replace_page(screensaver::keys(&panel));
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

    /// The screensaver picture as the settings choose it, drawn once per choice.
    fn screensaver_panel(&mut self) -> Option<Pixmap> {
        let image = &self.config.preferences.screensaver.image;
        if let Some((drawn_for, panel)) = &self.screensaver_drawn
            && drawn_for == image
        {
            return Some(panel.clone());
        }
        let panel = screensaver::draw(&self.screensaver_source()?, 1.0)?;
        self.screensaver_drawn = Some((image.clone(), panel.clone()));
        Some(panel)
    }

    /// Gives the dock its own copy of the screensaver, which it shows while the app is away.
    /// Drawn on its own thread: a large picture takes a while and the keys must not wait.
    pub(super) fn store_screensaver(&self) {
        let Some(source) = self.screensaver_source() else {
            return;
        };
        let dock = self.dock.clone();
        let uploads = Arc::clone(&self.screensaver_uploads);
        let mine = uploads.fetch_add(1, Ordering::SeqCst) + 1;
        let spawned = thread::Builder::new()
            .name("screensaver".into())
            .spawn(move || {
                let png = screensaver::draw(&source, 1.0).and_then(|panel| panel.encode_png().ok());
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
            self.show_current_page();
        }
        self.gestures.set_timing(timing(&self.config.preferences));
        self.save()?;
        let after = &self.config.preferences.screensaver;
        let new_image = after.image != before.image;
        let new_brightness = after.brightness != before.brightness;
        if new_image {
            self.store_screensaver();
        }
        match self.screen.reconsider(&self.rules(), Instant::now()) {
            Some(change) => self.apply_screen(change),
            // A screensaver on the dock shows a new image or brightness at once.
            None if self.screen.showing_screensaver() && (new_image || new_brightness) => {
                self.apply_screen(Change::Screensaver);
            }
            None => {}
        }
        Ok(())
    }

    /// Shows the screensaver on the dock for a while, or stops showing it.
    pub(super) fn try_screensaver(&mut self, on: bool) {
        let change = if on {
            self.screen.try_it(Instant::now())
        } else {
            self.screen.stop_trial()
        };
        if let Some(change) = change {
            self.apply_screen(change);
        }
    }
}

//! Edits the user can undo, and undoing and redoing them.

use super::commands::CommandError;
use super::history::Target;
use super::runtime::Engine;

impl Engine {
    /// Runs an edit and, if it changed anything, remembers how things were before it.
    pub(super) fn edit<T>(
        &mut self,
        target: Target,
        change: impl FnOnce(&mut Self) -> Result<T, CommandError>,
    ) -> Result<T, CommandError> {
        let before = self.config.profiles.clone();
        let result = change(self);
        if result.is_ok() && self.config.profiles != before {
            self.history.record(before, target);
            self.forget_stale_states();
            self.changed = true;
        }
        result
    }

    pub(super) fn undo(&mut self) -> Result<(), CommandError> {
        let current = self.config.profiles.clone();
        match self.history.undo(current) {
            Some(profiles) => self.restore(profiles),
            None => Ok(()),
        }
    }

    pub(super) fn redo(&mut self) -> Result<(), CommandError> {
        let current = self.config.profiles.clone();
        match self.history.redo(current) {
            Some(profiles) => self.restore(profiles),
            None => Ok(()),
        }
    }

    fn restore(&mut self, profiles: Vec<crate::config::Profile>) -> Result<(), CommandError> {
        let active = self.config.active_profile;
        self.config.profiles = profiles;
        if self.config.active_profile().is_none()
            && let Some(first) = self.config.profiles.first()
        {
            self.config.active_profile = first.id;
        }
        self.profiles_restored(active);
        self.save()?;
        self.forget_stale_states();
        self.show_current_page();
        Ok(())
    }
}

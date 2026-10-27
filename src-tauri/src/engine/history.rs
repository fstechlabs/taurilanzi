//! Undo and redo of what the user edits: keys, pages, lines. Whole copies of the profiles, kept
//! in memory while the app runs; a profile is a few KB, so this is cheap.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::config::Profile;

const MAX_STEPS: usize = 200;
/// Edits of the same thing closer than this are one step, so typing a title is undone at once.
const MERGE_WINDOW: Duration = Duration::from_secs(1);

/// What an edit touched, so repeated edits of the same thing can merge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Key { page: uuid::Uuid, index: u8 },
    Page(uuid::Uuid),
    Other,
}

struct Step {
    profiles: Vec<Profile>,
    target: Target,
    at: Instant,
}

#[derive(Default)]
pub struct History {
    undo: VecDeque<Step>,
    redo: Vec<Step>,
}

impl History {
    /// Remembers how things were before an edit. A new edit drops what could be redone.
    pub fn record(&mut self, before: Vec<Profile>, target: Target) {
        self.redo.clear();
        if let Some(last) = self.undo.back_mut()
            && target != Target::Other
            && last.target == target
            && last.at.elapsed() < MERGE_WINDOW
        {
            last.at = Instant::now();
            return;
        }
        self.undo.push_back(Step {
            profiles: before,
            target,
            at: Instant::now(),
        });
        if self.undo.len() > MAX_STEPS {
            self.undo.pop_front();
        }
    }

    /// Takes the profiles to go back to, keeping `current` to come back with redo.
    pub fn undo(&mut self, current: Vec<Profile>) -> Option<Vec<Profile>> {
        let step = self.undo.pop_back()?;
        self.redo.push(Step {
            profiles: current,
            target: Target::Other,
            at: Instant::now(),
        });
        Some(step.profiles)
    }

    pub fn redo(&mut self, current: Vec<Profile>) -> Option<Vec<Profile>> {
        let step = self.redo.pop()?;
        self.undo.push_back(Step {
            profiles: current,
            target: Target::Other,
            at: Instant::now(),
        });
        Some(step.profiles)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Every version kept, so images they use are not deleted as unused.
    pub fn versions(&self) -> impl Iterator<Item = &[Profile]> {
        self.undo
            .iter()
            .chain(&self.redo)
            .map(|step| step.profiles.as_slice())
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    fn profiles(name: &str) -> Vec<Profile> {
        vec![Profile {
            id: Uuid::nil(),
            name: name.into(),
            home_page: Uuid::nil(),
            pages: Vec::new(),
            apps: Vec::new(),
        }]
    }

    #[test]
    fn undoes_and_redoes_in_order() {
        let mut history = History::default();
        history.record(profiles("a"), Target::Other);
        history.record(profiles("b"), Target::Other);
        assert_eq!(history.undo(profiles("c")).unwrap()[0].name, "b");
        assert_eq!(history.undo(profiles("b")).unwrap()[0].name, "a");
        assert!(history.undo(profiles("a")).is_none());
        assert_eq!(history.redo(profiles("a")).unwrap()[0].name, "b");
        assert_eq!(history.redo(profiles("b")).unwrap()[0].name, "c");
        assert!(!history.can_redo());
    }

    #[test]
    fn quick_edits_of_one_key_are_one_step_and_new_edits_drop_redo() {
        let mut history = History::default();
        let key = Target::Key {
            page: Uuid::nil(),
            index: 3,
        };
        history.record(profiles("before typing"), key);
        history.record(profiles("after one letter"), key);
        history.record(profiles("after two letters"), key);
        assert_eq!(
            history.undo(profiles("typed")).unwrap()[0].name,
            "before typing"
        );
        assert!(!history.can_undo());
        history.record(profiles("something else"), Target::Other);
        assert!(!history.can_redo());
    }
}

//! Pages of the active profile as the canvas edits them.

use std::collections::BTreeMap;

use uuid::Uuid;

use super::commands::{CommandError, KeySlot, PageChanges};
use super::runtime::{Engine, check_index};
use crate::config::{Action, Key, Page, Position, back_key};

/// Where a new page's back key goes.
const BACK_KEY: u8 = 0;
const COPY_OFFSET: f32 = 48.0;
const MAX_NAME: usize = 60;

impl Engine {
    /// New pages come with a back key, so there is always a way out of them.
    pub(super) fn create_page(
        &mut self,
        name: Option<String>,
        position: Position,
    ) -> Result<Uuid, CommandError> {
        let back = back_key(&self.store.assets, &self.config.preferences.new_keys)?;
        let profile = self.profile_mut()?;
        let page = Page {
            id: Uuid::new_v4(),
            name: name
                .map(|n| clean_name(&n))
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| trf!("pages.numbered", number = profile.pages.len() + 1)),
            keys: BTreeMap::from([(BACK_KEY, back)]),
            position,
        };
        let id = page.id;
        profile.pages.push(page);
        self.save()?;
        Ok(id)
    }

    pub(super) fn update_page(
        &mut self,
        id: Uuid,
        changes: PageChanges,
    ) -> Result<(), CommandError> {
        let page = self
            .profile_mut()?
            .page_mut(id)
            .ok_or(CommandError::UnknownPage(id))?;
        if let Some(name) = changes
            .name
            .map(|n| clean_name(&n))
            .filter(|n| !n.is_empty())
        {
            page.name = name;
        }
        if let Some(position) = changes.position {
            page.position = position;
        }
        self.save()
    }

    /// The copy keeps the keys and their lines, and sits a little below and to the right.
    pub(super) fn duplicate_page(&mut self, id: Uuid) -> Result<Uuid, CommandError> {
        let profile = self.profile_mut()?;
        let original = profile.page(id).ok_or(CommandError::UnknownPage(id))?;
        let mut copy = Page {
            id: Uuid::new_v4(),
            name: clean_name(&trf!("pages.copy", name = original.name)),
            keys: original.keys.clone(),
            position: Position {
                x: original.position.x + COPY_OFFSET,
                y: original.position.y + COPY_OFFSET,
            },
        };
        Self::renew_plugin_ids(copy.keys.values_mut());
        let copy_id = copy.id;
        profile.pages.push(copy);
        self.save()?;
        Ok(copy_id)
    }

    /// Lines into the page go with it: keys that led there stop navigating.
    pub(super) fn delete_page(&mut self, id: Uuid) -> Result<(), CommandError> {
        let on_screen = self.navigation.current(self.profile()?) == id;
        let profile = self.profile_mut()?;
        if profile.home_page == id {
            return Err(CommandError::DeleteHome);
        }
        let before = profile.pages.len();
        profile.pages.retain(|page| page.id != id);
        if profile.pages.len() == before {
            return Err(CommandError::UnknownPage(id));
        }
        for key in profile.pages.iter_mut().flat_map(|p| p.keys.values_mut()) {
            key.actions
                .retain(|action| !matches!(action, Action::OpenPage { page, .. } if *page == id));
        }
        self.save()?;
        self.navigation.forget(id);
        self.save_navigation();
        if on_screen {
            self.show_current_page();
        }
        Ok(())
    }

    pub(super) fn set_home_page(&mut self, id: Uuid) -> Result<(), CommandError> {
        let profile = self.profile_mut()?;
        if profile.page(id).is_none() {
            return Err(CommandError::UnknownPage(id));
        }
        profile.home_page = id;
        self.save()
    }

    /// Swaps with whatever is at the destination, so dropping a key on another never loses one.
    pub(super) fn move_key(&mut self, from: KeySlot, to: KeySlot) -> Result<(), CommandError> {
        check_index(from.index)?;
        check_index(to.index)?;
        if from == to {
            return Ok(());
        }
        let profile = self.profile_mut()?;
        for page in [from.page, to.page] {
            if profile.page(page).is_none() {
                return Err(CommandError::UnknownPage(page));
            }
        }
        let Some(mut moving) = profile
            .page_mut(from.page)
            .and_then(|p| p.keys.remove(&from.index))
        else {
            return Ok(());
        };
        let displaced = profile
            .page_mut(to.page)
            .and_then(|p| p.keys.remove(&to.index));
        drop_link_to(&mut moving, to.page);
        if let Some(page) = profile.page_mut(to.page) {
            page.keys.insert(to.index, moving);
        }
        if let Some(mut displaced) = displaced {
            drop_link_to(&mut displaced, from.page);
            if let Some(page) = profile.page_mut(from.page) {
                page.keys.insert(from.index, displaced);
            }
        }
        self.save()?;
        let on_screen = self.navigation.current(self.profile()?);
        if on_screen == from.page || on_screen == to.page {
            self.show_current_page();
        }
        Ok(())
    }

    pub(super) fn show_page(&mut self, id: Uuid) -> Result<(), CommandError> {
        if self.profile()?.page(id).is_none() {
            return Err(CommandError::UnknownPage(id));
        }
        self.navigate(|nav, profile| nav.open(profile, id));
        Ok(())
    }
}

/// A key moved onto the page it leads to would lead nowhere.
fn drop_link_to(key: &mut Key, page: Uuid) {
    key.actions.retain(
        |action| !matches!(action, Action::OpenPage { page: target, .. } if *target == page),
    );
}

fn clean_name(name: &str) -> String {
    name.trim().chars().take(MAX_NAME).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Appearance;
    use crate::render::{Background, Color};

    #[test]
    fn moving_a_key_onto_its_target_drops_the_line() {
        let (here, there) = (Uuid::new_v4(), Uuid::new_v4());
        let mut key = Key {
            appearance: Appearance {
                background: Background::Solid {
                    color: Color::BLACK,
                },
                layers: Vec::new(),
            },
            actions: vec![
                Action::OpenPage {
                    page: there,
                    waypoints: vec![Position { x: 10.0, y: 20.0 }],
                },
                Action::OpenUrl {
                    url: "https://example.com".into(),
                },
            ],
            double_press: Vec::new(),
            long_press: Vec::new(),
            second_state: None,
        };
        drop_link_to(&mut key, here);
        assert_eq!(key.actions.len(), 2, "lines elsewhere stay");
        drop_link_to(&mut key, there);
        assert!(matches!(key.actions[..], [Action::OpenUrl { .. }]));
    }
}

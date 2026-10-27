use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::model::Profile;

/// Longer paths drop their oldest steps after the first one.
const MAX_DEPTH: usize = 32;

/// Where the dock is in each profile: the pages visited to reach the current one, so Back
/// returns to wherever the user came from. Runtime state, kept apart from the configuration.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Navigation {
    paths: BTreeMap<Uuid, Vec<Uuid>>,
}

impl Navigation {
    /// The path of `profile` without pages that no longer exist; never empty.
    pub fn path(&self, profile: &Profile) -> Vec<Uuid> {
        let mut path: Vec<Uuid> = self
            .paths
            .get(&profile.id)
            .into_iter()
            .flatten()
            .copied()
            .filter(|id| profile.page(*id).is_some())
            .collect();
        path.dedup();
        if path.is_empty() {
            path.push(profile.home_page);
        }
        path
    }

    pub fn current(&self, profile: &Profile) -> Uuid {
        *self.path(profile).last().expect("path is never empty")
    }

    /// Going to a page already on the path goes back to it, so loops do not grow the path.
    pub fn open(&mut self, profile: &Profile, page: Uuid) -> bool {
        let mut path = self.path(profile);
        match path.iter().position(|id| *id == page) {
            Some(index) => path.truncate(index + 1),
            None => {
                path.push(page);
                if path.len() > MAX_DEPTH {
                    path.remove(1);
                }
            }
        }
        self.set(profile, path)
    }

    pub fn back(&mut self, profile: &Profile) -> bool {
        let mut path = self.path(profile);
        if path.len() > 1 {
            path.pop();
        }
        self.set(profile, path)
    }

    pub fn home(&mut self, profile: &Profile) -> bool {
        self.set(profile, vec![profile.home_page])
    }

    /// Drops a deleted page from every path.
    pub fn forget(&mut self, page: Uuid) {
        for path in self.paths.values_mut() {
            path.retain(|id| *id != page);
        }
    }

    /// Returns whether the page on screen changed.
    fn set(&mut self, profile: &Profile, path: Vec<Uuid>) -> bool {
        let before = self.current(profile);
        self.paths.insert(profile.id, path);
        self.current(profile) != before
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::config::model::{Page, Position};

    fn profile(pages: usize) -> Profile {
        let pages: Vec<Page> = (0..pages)
            .map(|i| Page {
                id: Uuid::new_v4(),
                name: format!("P{i}"),
                keys: BTreeMap::new(),
                position: Position::default(),
            })
            .collect();
        Profile {
            id: Uuid::new_v4(),
            name: "Test".into(),
            home_page: pages[0].id,
            pages,
            apps: Vec::new(),
        }
    }

    #[test]
    fn back_returns_to_where_the_user_came_from() {
        let p = profile(4);
        let [home, a, b, shared] = [0, 1, 2, 3].map(|i| p.pages[i].id);
        let mut nav = Navigation::default();
        assert_eq!(nav.current(&p), home);

        nav.open(&p, a);
        nav.open(&p, shared);
        nav.back(&p);
        assert_eq!(nav.current(&p), a);

        nav.home(&p);
        nav.open(&p, b);
        nav.open(&p, shared);
        nav.back(&p);
        assert_eq!(nav.current(&p), b, "same page, reached from elsewhere");
        nav.back(&p);
        nav.back(&p);
        assert_eq!(nav.current(&p), home, "back stops at the start");
    }

    #[test]
    fn loops_cut_the_path_instead_of_growing_it() {
        let p = profile(3);
        let [home, a, b] = [0, 1, 2].map(|i| p.pages[i].id);
        let mut nav = Navigation::default();
        for _ in 0..10 {
            nav.open(&p, a);
            nav.open(&p, b);
        }
        assert_eq!(nav.path(&p), [home, a, b]);
        assert!(!nav.open(&p, b), "already there");
    }

    #[test]
    fn deleted_pages_drop_out_of_the_path() {
        let mut p = profile(3);
        let [home, a, b] = [0, 1, 2].map(|i| p.pages[i].id);
        let mut nav = Navigation::default();
        nav.open(&p, a);
        nav.open(&p, b);
        p.pages.retain(|page| page.id != b);
        assert_eq!(nav.current(&p), a);
        p.pages.retain(|page| page.id != a);
        assert_eq!(nav.path(&p), [home]);
    }

    #[test]
    fn deep_paths_keep_their_start() {
        let p = profile(MAX_DEPTH + 5);
        let mut nav = Navigation::default();
        for page in &p.pages[1..] {
            nav.open(&p, page.id);
        }
        let path = nav.path(&p);
        assert_eq!(path.len(), MAX_DEPTH);
        assert_eq!(path[0], p.home_page);
        assert_eq!(*path.last().unwrap(), p.pages.last().unwrap().id);
    }
}

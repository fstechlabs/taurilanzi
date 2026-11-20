//! Where the rules look for the images and sounds keys point to.

use std::collections::HashSet;

use crate::config::assets::AssetStore;
use crate::config::model::{Profile, assets_in};

pub trait Assets {
    fn has_image(&self, id: &str) -> bool;
    fn has_sound(&self, id: &str) -> bool;
}

impl Assets for AssetStore {
    fn has_image(&self, id: &str) -> bool {
        AssetStore::has_image(self, id)
    }

    fn has_sound(&self, id: &str) -> bool {
        AssetStore::has_sound(self, id)
    }
}

/// The store, and every file the configuration already points to: a key synced from another
/// computer may arrive before its files do, and must still be editable.
pub struct KnownAssets<'a> {
    store: &'a AssetStore,
    known: HashSet<&'a str>,
}

impl<'a> KnownAssets<'a> {
    pub fn new(store: &'a AssetStore, profiles: &'a [Profile]) -> Self {
        Self {
            store,
            known: assets_in(profiles),
        }
    }
}

impl Assets for KnownAssets<'_> {
    fn has_image(&self, id: &str) -> bool {
        self.store.has_image(id) || self.known.contains(id)
    }

    fn has_sound(&self, id: &str) -> bool {
        self.store.has_sound(id) || self.known.contains(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::first_run;

    #[test]
    fn files_the_configuration_points_to_count_before_they_arrive() {
        let dir = tempfile::tempdir().unwrap();
        let store = AssetStore::open(dir.path().to_path_buf()).unwrap();
        let config = first_run(&store).unwrap();
        let used: Vec<String> = assets_in(&config.profiles)
            .into_iter()
            .map(str::to_owned)
            .collect();
        assert!(!used.is_empty(), "the first keys have images");
        let other = tempfile::tempdir().unwrap();
        let empty = AssetStore::open(other.path().to_path_buf()).unwrap();
        let known = KnownAssets::new(&empty, &config.profiles);
        for id in &used {
            assert!(!Assets::has_image(&empty, id));
            assert!(known.has_image(id), "{id} is in a key already");
        }
        assert!(!known.has_image(&format!("{}.png", "0".repeat(64))));
    }
}

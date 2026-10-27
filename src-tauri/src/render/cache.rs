use std::collections::HashMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

const CAPACITY: usize = 512;

/// PNGs keyed by a hash of the SVG and canvas size.
#[derive(Default)]
pub struct RenderCache {
    entries: HashMap<u64, Arc<Vec<u8>>>,
}

impl RenderCache {
    pub fn key(svg: &str, size: (u32, u32)) -> u64 {
        let mut hasher = DefaultHasher::new();
        svg.hash(&mut hasher);
        size.hash(&mut hasher);
        hasher.finish()
    }

    pub fn get(&self, key: u64) -> Option<Arc<Vec<u8>>> {
        self.entries.get(&key).cloned()
    }

    pub fn insert(&mut self, key: u64, png: Arc<Vec<u8>>) {
        // Simple bound: renders are cheap, so dropping everything now and then is fine.
        if self.entries.len() >= CAPACITY {
            self.entries.clear();
        }
        self.entries.insert(key, png);
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

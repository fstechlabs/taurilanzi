//! The clients of the servers in the configuration, one per server, shared by the engine, the
//! actions and the IPC. The engine keeps the set up to date with the configuration.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use uuid::Uuid;

#[derive(Clone)]
pub struct Clients<T>(Arc<RwLock<HashMap<Uuid, T>>>);

impl<T> Default for Clients<T> {
    fn default() -> Self {
        Self(Arc::default())
    }
}

impl<T: Clone> Clients<T> {
    pub fn get(&self, server: Uuid) -> Option<T> {
        self.0.read().expect("clients").get(&server).cloned()
    }

    pub fn contains(&self, server: Uuid) -> bool {
        self.0.read().expect("clients").contains_key(&server)
    }

    pub fn insert(&self, server: Uuid, client: T) {
        self.0.write().expect("clients").insert(server, client);
    }

    /// Drops the clients of servers not in `keep`, which stops them once nothing else holds them.
    pub fn retain(&self, keep: impl Fn(Uuid) -> bool) {
        self.0.write().expect("clients").retain(|id, _| keep(*id));
    }
}

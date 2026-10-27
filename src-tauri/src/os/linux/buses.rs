//! One connection to each D-Bus bus, shared by everything that asks the desktop.

use std::sync::{Mutex, PoisonError};

use zbus::blocking::Connection;

static SESSION: Mutex<Option<Connection>> = Mutex::new(None);
static SYSTEM: Mutex<Option<Connection>> = Mutex::new(None);

/// The user's bus: the keyring, the media players, the desktop shell.
pub fn session_bus() -> Option<Connection> {
    shared(&SESSION, Connection::session)
}

/// The machine's bus: the login sessions.
pub fn system_bus() -> Option<Connection> {
    shared(&SYSTEM, Connection::system)
}

/// Connects again the next time if it failed, as the bus may come later than the app.
fn shared(
    slot: &Mutex<Option<Connection>>,
    connect: fn() -> zbus::Result<Connection>,
) -> Option<Connection> {
    let mut slot = slot.lock().unwrap_or_else(PoisonError::into_inner);
    if slot.is_none() {
        match connect() {
            Ok(connection) => *slot = Some(connection),
            Err(err) => log::debug!("d-bus unavailable: {err}"),
        }
    }
    slot.clone()
}

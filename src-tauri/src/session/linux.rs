//! logind knows whether the desktop locked the user's session.

use zbus::blocking::Proxy;

use crate::os::linux::system_bus;

pub fn screen_locked() -> Option<bool> {
    let bus = system_bus()?;
    let session = Proxy::new(
        &bus,
        "org.freedesktop.login1",
        "/org/freedesktop/login1/session/auto",
        "org.freedesktop.login1.Session",
    )
    .ok()?;
    session.get_property::<bool>("LockedHint").ok()
}

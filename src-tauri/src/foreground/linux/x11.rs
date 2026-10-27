//! X11 keeps the window in front on the root window, for every app to read.

use x11rb::connection::Connection;
use x11rb::protocol::xproto::{Atom, AtomEnum, ConnectionExt, Window};
use x11rb::rust_connection::RustConnection;

use super::apps;
use super::backend::Front;
use crate::config::AppRef;

pub struct X11 {
    connection: RustConnection,
    root: Window,
    active: Atom,
    pid: Atom,
    /// The last window read, so the same one is not looked up again every poll.
    last: Option<(Window, u32, Option<AppRef>)>,
}

impl X11 {
    pub fn connect() -> Option<Self> {
        let (connection, screen) = x11rb::connect(None).ok()?;
        let root = connection.setup().roots.get(screen)?.root;
        let atom = |name: &[u8]| {
            connection
                .intern_atom(false, name)
                .ok()?
                .reply()
                .ok()
                .map(|reply| reply.atom)
        };
        let active = atom(b"_NET_ACTIVE_WINDOW")?;
        let pid = atom(b"_NET_WM_PID")?;
        Some(Self {
            connection,
            root,
            active,
            pid,
            last: None,
        })
    }

    pub fn frontmost(&mut self) -> Option<Front> {
        let window = self.window(self.root, self.active, AtomEnum::WINDOW)?;
        if window == 0 {
            return None;
        }
        if let Some((last, pid, app)) = &self.last
            && *last == window
        {
            return Some(Front {
                pid: *pid,
                app: app.clone(),
            });
        }
        let pid = self
            .window(window, self.pid, AtomEnum::CARDINAL)
            .unwrap_or(0);
        let app = (pid != 0)
            .then(|| apps::of_process(pid))
            .flatten()
            .or_else(|| self.class(window).and_then(|class| apps::named(&class)));
        self.last = Some((window, pid, app.clone()));
        Some(Front { pid, app })
    }

    fn window(&self, window: Window, property: Atom, kind: AtomEnum) -> Option<u32> {
        let reply = self
            .connection
            .get_property(false, window, property, kind, 0, 1)
            .ok()?
            .reply()
            .ok()?;
        reply.value32()?.next()
    }

    /// `WM_CLASS` holds the instance and then the class, each ended by a null.
    fn class(&self, window: Window) -> Option<String> {
        let reply = self
            .connection
            .get_property(false, window, AtomEnum::WM_CLASS, AtomEnum::STRING, 0, 256)
            .ok()?
            .reply()
            .ok()?;
        let text = String::from_utf8_lossy(&reply.value).into_owned();
        let mut parts = text.split('\0').filter(|part| !part.is_empty());
        let instance = parts.next().map(str::to_owned);
        parts.next().map(str::to_owned).or(instance)
    }
}

//! Sway, Hyprland and other wlroots desktops list their windows to clients through the
//! wlr-foreign-toplevel protocol, with the one in front marked as activated. It names apps, not
//! processes.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use wayland_client::backend::ObjectId;
use wayland_client::protocol::wl_registry;
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, event_created_child};
use wayland_protocols_wlr::foreign_toplevel::v1::client::{
    zwlr_foreign_toplevel_handle_v1::{self, ZwlrForeignToplevelHandleV1},
    zwlr_foreign_toplevel_manager_v1::{self, ZwlrForeignToplevelManagerV1},
};

use super::apps;
use super::backend::Front;

type Latest = Arc<Mutex<Option<String>>>;

pub struct Wlroots {
    latest: Latest,
}

#[derive(Default)]
struct Window {
    app_id: String,
    activated: bool,
}

struct Tracker {
    latest: Latest,
    windows: HashMap<ObjectId, Window>,
    found: bool,
}

impl Wlroots {
    pub fn start() -> Option<Self> {
        let connection = Connection::connect_to_env().ok()?;
        let mut queue = connection.new_event_queue();
        let handle = queue.handle();
        connection.display().get_registry(&handle, ());
        let latest: Latest = Arc::default();
        let mut tracker = Tracker {
            latest: latest.clone(),
            windows: HashMap::new(),
            found: false,
        };
        queue.roundtrip(&mut tracker).ok()?;
        if !tracker.found {
            return None;
        }
        std::thread::Builder::new()
            .name("wlr-toplevels".into())
            .spawn(move || {
                let _connection = connection;
                while queue.blocking_dispatch(&mut tracker).is_ok() {}
            })
            .ok()?;
        Some(Self { latest })
    }

    pub fn frontmost(&self) -> Option<Front> {
        let id = self
            .latest
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()?;
        Some(Front {
            pid: 0,
            app: apps::named(&id),
        })
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for Tracker {
    fn event(
        tracker: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        handle: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
            && interface == ZwlrForeignToplevelManagerV1::interface().name
        {
            registry.bind::<ZwlrForeignToplevelManagerV1, _, _>(name, version.min(3), handle, ());
            tracker.found = true;
        }
    }
}

impl Dispatch<ZwlrForeignToplevelManagerV1, ()> for Tracker {
    fn event(
        _: &mut Self,
        _: &ZwlrForeignToplevelManagerV1,
        _: zwlr_foreign_toplevel_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }

    event_created_child!(Tracker, ZwlrForeignToplevelManagerV1, [
        zwlr_foreign_toplevel_manager_v1::EVT_TOPLEVEL_OPCODE => (ZwlrForeignToplevelHandleV1, ()),
    ]);
}

impl Dispatch<ZwlrForeignToplevelHandleV1, ()> for Tracker {
    fn event(
        tracker: &mut Self,
        toplevel: &ZwlrForeignToplevelHandleV1,
        event: zwlr_foreign_toplevel_handle_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let id = toplevel.id();
        match event {
            zwlr_foreign_toplevel_handle_v1::Event::AppId { app_id } => {
                tracker.windows.entry(id).or_default().app_id = app_id;
            }
            zwlr_foreign_toplevel_handle_v1::Event::State { state } => {
                let activated = state
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|bytes| u32::from_ne_bytes(*bytes))
                    .any(|value| value == zwlr_foreign_toplevel_handle_v1::State::Activated as u32);
                tracker.windows.entry(id).or_default().activated = activated;
            }
            zwlr_foreign_toplevel_handle_v1::Event::Done => {
                if let Some(window) = tracker.windows.get(&id)
                    && window.activated
                    && !window.app_id.is_empty()
                {
                    *tracker
                        .latest
                        .lock()
                        .unwrap_or_else(PoisonError::into_inner) = Some(window.app_id.clone());
                }
            }
            zwlr_foreign_toplevel_handle_v1::Event::Closed => {
                tracker.windows.remove(&id);
                toplevel.destroy();
            }
            _ => {}
        }
    }
}

//! What the engine and a dock's driver say to each other, whatever the family: requests go to the
//! driver's worker through a `Dock`, events come back tagged with the connection they are about.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};

use log::warn;

use super::device::DeviceInfo;
use super::model::DeviceModel;

/// One device plugged in, from the moment it is found until it goes; the same device plugged
/// in again is another connection.
pub type ConnectionId = u64;

static NEXT_CONNECTION: AtomicU64 = AtomicU64::new(1);

pub(super) fn next_connection() -> ConnectionId {
    NEXT_CONNECTION.fetch_add(1, Ordering::Relaxed)
}

pub(super) enum Request {
    Brightness(u8),
    SleepMinutes(u32),
    SetKeys(BTreeMap<u8, Vec<u8>>),
    ReplacePage(BTreeMap<u8, Vec<u8>>),
    Screensaver(Vec<u8>),
}

/// Handle to the worker of one connected device. Dropping every copy stops the worker.
#[derive(Clone)]
pub struct Dock {
    requests: Sender<Request>,
}

impl Dock {
    pub(super) fn channel() -> (Self, Receiver<Request>) {
        let (requests, receiver) = mpsc::channel();
        (Self { requests }, receiver)
    }

    pub fn set_brightness(&self, percent: u8) {
        self.send(Request::Brightness(percent.min(100)));
    }

    /// `0` disables sleep.
    pub fn set_sleep_minutes(&self, minutes: u32) {
        self.send(Request::SleepMinutes(minutes));
    }

    /// Updates some keys with images in the model's format, leaving the others untouched.
    pub fn set_keys(&self, images: BTreeMap<u8, Vec<u8>>) {
        self.send(Request::SetKeys(images));
    }

    /// Replaces the whole page; keys without an image are cleared.
    pub fn replace_page(&self, images: BTreeMap<u8, Vec<u8>>) {
        self.send(Request::ReplacePage(images));
    }

    /// Stores the image a device that keeps one shows by itself while nobody talks to it: a PNG
    /// of the whole panel.
    pub fn set_screensaver(&self, png: Vec<u8>) {
        self.send(Request::Screensaver(png));
    }

    fn send(&self, request: Request) {
        if self.requests.send(request).is_err() {
            warn!("the dock's worker is not running");
        }
    }
}

impl std::fmt::Debug for Dock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Dock")
    }
}

#[derive(Debug)]
pub enum Event {
    /// A device was found and its worker started; nothing is known of it yet.
    Attached {
        model: &'static DeviceModel,
        dock: Dock,
        /// It exists only in the app.
        simulated: bool,
    },
    /// The device told who it is, and takes images from now on.
    Connected(DeviceInfo),
    Key {
        index: u8,
        pressed: bool,
    },
    Disconnected,
}

#[derive(Debug)]
pub struct DockEvent {
    pub connection: ConnectionId,
    pub event: Event,
}

/// How a worker reports what happens to its connection.
#[derive(Clone)]
pub(super) struct Reporter {
    pub connection: ConnectionId,
    events: Sender<DockEvent>,
}

impl Reporter {
    pub fn new(connection: ConnectionId, events: Sender<DockEvent>) -> Self {
        Self { connection, events }
    }

    /// Returns false when nobody listens anymore.
    pub fn send(&self, event: Event) -> bool {
        self.events
            .send(DockEvent {
                connection: self.connection,
                event,
            })
            .is_ok()
    }
}

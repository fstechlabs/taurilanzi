//! A keyboard of the app's own through uinput, which the kernel passes on like a real one: it
//! works the same under X11 and every Wayland desktop, which give apps no other way to press keys.

use std::io;
use std::sync::{Mutex, PoisonError};
use std::thread;
use std::time::Duration;

use evdev::uinput::VirtualDevice;
use evdev::{AttributeSet, EventType, InputEvent, KeyCode};

use super::keycodes::CODES;
use crate::actions::error::ActionError;

pub const UINPUT: &str = "/dev/uinput";
/// The desktop takes a moment to start listening to a new keyboard; keys sent before are lost.
const SETTLE: Duration = Duration::from_millis(300);
/// Some apps read keys pressed and released at once as nothing.
const BETWEEN_EVENTS: Duration = Duration::from_millis(4);

pub const EXTRA_KEYS: &[KeyCode] = &[
    KeyCode::KEY_LEFTCTRL,
    KeyCode::KEY_LEFTSHIFT,
    KeyCode::KEY_LEFTALT,
    KeyCode::KEY_RIGHTALT,
    KeyCode::KEY_LEFTMETA,
    KeyCode::KEY_VOLUMEUP,
    KeyCode::KEY_VOLUMEDOWN,
    KeyCode::KEY_MUTE,
    KeyCode::KEY_PLAYPAUSE,
    KeyCode::KEY_NEXTSONG,
    KeyCode::KEY_PREVIOUSSONG,
    KeyCode::KEY_BRIGHTNESSUP,
    KeyCode::KEY_BRIGHTNESSDOWN,
];

static DEVICE: Mutex<Option<VirtualDevice>> = Mutex::new(None);

pub struct Keyboard<'a> {
    device: &'a mut VirtualDevice,
}

impl Keyboard<'_> {
    pub fn key(&mut self, code: KeyCode, down: bool) -> io::Result<()> {
        self.device.emit(&[InputEvent::new(
            EventType::KEY.0,
            code.code(),
            i32::from(down),
        )])?;
        thread::sleep(BETWEEN_EVENTS);
        Ok(())
    }

    /// Holds the modifiers, taps the key and lets them go, last pressed first released.
    pub fn chord(&mut self, modifiers: &[KeyCode], key: KeyCode) -> io::Result<()> {
        for &modifier in modifiers {
            self.key(modifier, true)?;
        }
        self.key(key, true)?;
        self.key(key, false)?;
        for &modifier in modifiers.iter().rev() {
            self.key(modifier, false)?;
        }
        Ok(())
    }
}

/// Runs `keys` with the keyboard, creating it the first time.
pub fn with_keyboard(
    keys: impl FnOnce(&mut Keyboard) -> io::Result<()>,
) -> Result<(), ActionError> {
    let mut device = DEVICE.lock().unwrap_or_else(PoisonError::into_inner);
    if device.is_none() {
        *device = Some(create().map_err(|err| match err.kind() {
            io::ErrorKind::PermissionDenied | io::ErrorKind::NotFound => {
                ActionError::NoAccessibility
            }
            _ => ActionError::Launch(err),
        })?);
        thread::sleep(SETTLE);
    }
    let device = device.as_mut().expect("created above");
    keys(&mut Keyboard { device }).map_err(ActionError::Launch)
}

fn create() -> io::Result<VirtualDevice> {
    let mut keys = AttributeSet::<KeyCode>::new();
    for (_, code) in CODES {
        keys.insert(*code);
    }
    for code in EXTRA_KEYS {
        keys.insert(*code);
    }
    VirtualDevice::builder()?
        .name("Taurilanzi virtual keyboard")
        .with_keys(&keys)?
        .build()
}

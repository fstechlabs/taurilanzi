//! The user's keyboard layout, to type text with the keys that print each character on it: a
//! virtual keyboard sends key positions, and the desktop turns them into characters with the
//! layout, as it does for the real keyboard.

use std::collections::HashMap;
use std::os::fd::OwnedFd;

use evdev::KeyCode;
use unicode_normalization::UnicodeNormalization;
use wayland_client::protocol::{wl_keyboard, wl_registry, wl_seat};
use wayland_client::{Connection, Dispatch, QueueHandle, WEnum};
use xkbcommon::xkb;

/// xkb numbers keys 8 above the kernel.
const XKB_OFFSET: u32 = 8;

/// A key, with what must be held to get the character from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stroke {
    pub key: KeyCode,
    pub shift: bool,
    pub alt_gr: bool,
}

impl Stroke {
    fn effort(self) -> u8 {
        u8::from(self.shift) + 2 * u8::from(self.alt_gr)
    }
}

/// The strokes of each keysym of the first layout.
pub struct Layout {
    strokes: HashMap<u32, Stroke>,
}

impl Layout {
    /// The layout the desktop uses now, or the system's default one.
    pub fn current() -> Option<Self> {
        let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let keymap = std::env::var_os("WAYLAND_DISPLAY")
            .and_then(|_| wayland(&context))
            .or_else(|| x11(&context))
            .or_else(|| {
                xkb::Keymap::new_from_names(
                    &context,
                    "",
                    "",
                    "",
                    "",
                    None,
                    xkb::KEYMAP_COMPILE_NO_FLAGS,
                )
            })?;
        Some(Self::from_keymap(&keymap))
    }

    fn from_keymap(keymap: &xkb::Keymap) -> Self {
        let shift = 1u32 << keymap.mod_get_index(xkb::MOD_NAME_SHIFT);
        let level3 = 1u32 << keymap.mod_get_index(xkb::MOD_NAME_ISO_LEVEL3_SHIFT);
        let mut strokes: HashMap<u32, Stroke> = HashMap::new();
        keymap.key_for_each(|keymap, keycode| {
            let Some(raw) = keycode.raw().checked_sub(XKB_OFFSET) else {
                return;
            };
            let key = KeyCode::new(raw as u16);
            for level in 0..keymap.num_levels_for_key(keycode, 0) {
                let mut masks = [0u32; 8];
                let count = keymap.key_get_mods_for_level(keycode, 0, level, &mut masks);
                let Some(mask) = masks[..count.min(masks.len())]
                    .iter()
                    .copied()
                    .filter(|mask| mask & !(shift | level3) == 0)
                    .min_by_key(|mask| mask.count_ones())
                else {
                    continue;
                };
                let stroke = Stroke {
                    key,
                    shift: mask & shift != 0,
                    alt_gr: mask & level3 != 0,
                };
                if let [sym] = keymap.key_get_syms_by_level(keycode, 0, level) {
                    let known = strokes.get(&sym.raw());
                    if known.is_none_or(|k| stroke.effort() < k.effort()) {
                        strokes.insert(sym.raw(), stroke);
                    }
                }
            }
        });
        Self { strokes }
    }

    /// The strokes that type `text`, or None if a character is on no key of the layout.
    pub fn plan(&self, text: &str) -> Option<Vec<Stroke>> {
        let mut plan = Vec::new();
        for c in text.chars() {
            match c {
                '\r' => {}
                '\n' => plan.push(Stroke {
                    key: KeyCode::KEY_ENTER,
                    shift: false,
                    alt_gr: false,
                }),
                c => plan.extend(self.strokes_for(c)?),
            }
        }
        Some(plan)
    }

    /// A character on a key, or written with a dead key and then the letter (`´` then `a`).
    fn strokes_for(&self, c: char) -> Option<Vec<Stroke>> {
        if let Some(stroke) = self.stroke(c) {
            return Some(vec![stroke]);
        }
        let parts: Vec<char> = c.nfd().collect();
        let [base, mark] = parts[..] else {
            return None;
        };
        let dead = self.strokes.get(&dead_key(mark)?)?;
        Some(vec![*dead, self.stroke(base)?])
    }

    fn stroke(&self, c: char) -> Option<Stroke> {
        let sym = xkb::utf32_to_keysym(c as u32).raw();
        self.strokes.get(&sym).copied()
    }
}

/// The dead key that adds a combining mark, as keysyms name them.
fn dead_key(mark: char) -> Option<u32> {
    Some(match mark {
        '\u{0300}' => 0xfe50,
        '\u{0301}' => 0xfe51,
        '\u{0302}' => 0xfe52,
        '\u{0303}' => 0xfe53,
        '\u{0304}' => 0xfe54,
        '\u{0306}' => 0xfe55,
        '\u{0307}' => 0xfe56,
        '\u{0308}' => 0xfe57,
        '\u{030A}' => 0xfe58,
        '\u{030B}' => 0xfe59,
        '\u{030C}' => 0xfe5a,
        '\u{0327}' => 0xfe5b,
        '\u{0328}' => 0xfe5c,
        _ => return None,
    })
}

/// The compositor hands every client the keymap of the seat's keyboard, focused or not.
fn wayland(context: &xkb::Context) -> Option<xkb::Keymap> {
    let connection = Connection::connect_to_env().ok()?;
    let mut queue = connection.new_event_queue();
    let handle = queue.handle();
    connection.display().get_registry(&handle, ());
    let mut probe = Probe::default();
    for _ in 0..4 {
        queue.roundtrip(&mut probe).ok()?;
        if probe.keymap.is_some() {
            break;
        }
    }
    let (fd, size) = probe.keymap?;
    // SAFETY: the compositor shares this file for clients to map, and it stays as it is.
    unsafe {
        xkb::Keymap::new_from_fd(
            context,
            fd,
            size as usize,
            xkb::KEYMAP_FORMAT_TEXT_V1,
            xkb::KEYMAP_COMPILE_NO_FLAGS,
        )
    }
    .ok()?
}

#[derive(Default)]
struct Probe {
    keymap: Option<(OwnedFd, u32)>,
}

impl Dispatch<wl_registry::WlRegistry, ()> for Probe {
    fn event(
        _: &mut Self,
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
            && interface == "wl_seat"
        {
            registry.bind::<wl_seat::WlSeat, _, _>(name, version.min(5), handle, ());
        }
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for Probe {
    fn event(
        _: &mut Self,
        seat: &wl_seat::WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        handle: &QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Capabilities {
            capabilities: WEnum::Value(capabilities),
        } = event
            && capabilities.contains(wl_seat::Capability::Keyboard)
        {
            seat.get_keyboard(handle, ());
        }
    }
}

impl Dispatch<wl_keyboard::WlKeyboard, ()> for Probe {
    fn event(
        probe: &mut Self,
        _: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_keyboard::Event::Keymap {
            format: WEnum::Value(wl_keyboard::KeymapFormat::XkbV1),
            fd,
            size,
        } = event
            && probe.keymap.is_none()
        {
            probe.keymap = Some((fd, size));
        }
    }
}

/// An X server keeps the names of its layout on the root window, as `setxkbmap -query` shows.
fn x11(context: &xkb::Context) -> Option<xkb::Keymap> {
    use x11rb::connection::Connection as _;
    use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as _};

    let (connection, screen) = x11rb::connect(None).ok()?;
    let root = connection.setup().roots.get(screen)?.root;
    let atom = connection
        .intern_atom(true, b"_XKB_RULES_NAMES")
        .ok()?
        .reply()
        .ok()?
        .atom;
    let value = connection
        .get_property(false, root, atom, AtomEnum::STRING, 0, 1024)
        .ok()?
        .reply()
        .ok()?
        .value;
    let names = RuleNames::parse(&value)?;
    xkb::Keymap::new_from_names(
        context,
        &names.rules,
        &names.model,
        &names.layout,
        &names.variant,
        names.options,
        xkb::KEYMAP_COMPILE_NO_FLAGS,
    )
}

#[derive(Debug, PartialEq)]
struct RuleNames {
    rules: String,
    model: String,
    layout: String,
    variant: String,
    options: Option<String>,
}

impl RuleNames {
    /// Rules, model, layouts, variants and options, each ended by a null; only the first layout
    /// and its variant are kept.
    fn parse(value: &[u8]) -> Option<Self> {
        let text = std::str::from_utf8(value).ok()?;
        let mut fields = text.split('\0').map(str::to_owned);
        let rules = fields.next()?;
        let model = fields.next().unwrap_or_default();
        let layout = fields.next().unwrap_or_default();
        let variant = fields.next().unwrap_or_default();
        let options = fields.next().filter(|o| !o.is_empty());
        let first = |list: &str| list.split(',').next().unwrap_or_default().to_owned();
        Some(Self {
            rules,
            model,
            layout: first(&layout),
            variant: first(&variant),
            options,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_names_of_the_x_servers_layout() {
        let names =
            RuleNames::parse(b"evdev\0pc105\0es,us\0,intl\0grp:alt_shift_toggle\0").unwrap();
        assert_eq!(names.layout, "es");
        assert_eq!(names.variant, "");
        assert_eq!(names.options.as_deref(), Some("grp:alt_shift_toggle"));
    }

    fn spanish() -> Option<Layout> {
        let context = xkb::Context::new(xkb::CONTEXT_NO_FLAGS);
        let keymap = xkb::Keymap::new_from_names(
            &context,
            "evdev",
            "pc105",
            "es",
            "",
            None,
            xkb::KEYMAP_COMPILE_NO_FLAGS,
        )?;
        Some(Layout::from_keymap(&keymap))
    }

    #[test]
    fn types_with_the_keys_of_the_layout() {
        // Machines without the layout files cannot build keymaps; there is nothing to check.
        let Some(layout) = spanish() else {
            return;
        };
        let plan = layout.plan("Añ@é\n").unwrap();
        let a = Stroke {
            key: KeyCode::KEY_A,
            shift: true,
            alt_gr: false,
        };
        let n = Stroke {
            key: KeyCode::KEY_SEMICOLON,
            shift: false,
            alt_gr: false,
        };
        let at = Stroke {
            key: KeyCode::KEY_2,
            shift: false,
            alt_gr: true,
        };
        assert_eq!(&plan[..3], &[a, n, at]);
        assert_eq!(plan[3].key, KeyCode::KEY_APOSTROPHE, "the dead acute");
        assert_eq!(plan[4].key, KeyCode::KEY_E);
        assert_eq!(plan[5].key, KeyCode::KEY_ENTER);
        assert_eq!(layout.plan("😀"), None);
    }
}

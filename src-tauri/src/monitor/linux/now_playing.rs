//! The song any player plays, through MPRIS, which Linux players publish on D-Bus.

use std::collections::HashMap;
use std::time::Instant;

use zbus::blocking::Proxy;
use zbus::zvariant::OwnedValue;

use crate::monitor::feeds::Track;
use crate::os::linux::session_bus;

const PREFIX: &str = "org.mpris.MediaPlayer2.";
const MICROSECONDS: f32 = 1_000_000.0;

/// The song playing, or a paused one if none plays.
pub fn now_playing() -> Option<Track> {
    let bus = session_bus()?;
    let names: Vec<String> = Proxy::new(
        &bus,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
    )
    .ok()?
    .call("ListNames", &())
    .ok()?;
    let mut found: Vec<Track> = names
        .iter()
        .filter(|name| name.starts_with(PREFIX))
        .filter_map(|name| player(&bus, name))
        .collect();
    let playing = found.iter().position(|t| t.playing).unwrap_or(0);
    (!found.is_empty()).then(|| found.swap_remove(playing))
}

fn player(bus: &zbus::blocking::Connection, name: &str) -> Option<Track> {
    let player = Proxy::new(
        bus,
        name,
        "/org/mpris/MediaPlayer2",
        "org.mpris.MediaPlayer2.Player",
    )
    .ok()?;
    let playing = match player
        .get_property::<String>("PlaybackStatus")
        .ok()?
        .as_str()
    {
        "Playing" => true,
        "Paused" => false,
        _ => return None,
    };
    let metadata: HashMap<String, OwnedValue> = player.get_property("Metadata").ok()?;
    let title = text(metadata.get("xesam:title")?)?;
    if title.is_empty() {
        return None;
    }
    let artist = metadata
        .get("xesam:artist")
        .and_then(|value| Vec::<String>::try_from(value.try_clone().ok()?).ok())
        .map(|artists| artists.join(", "))
        .unwrap_or_default();
    let duration = metadata
        .get("mpris:length")
        .and_then(microseconds)
        .filter(|d| *d > 0.0);
    let position = player
        .get_property::<i64>("Position")
        .ok()
        .map(|p| p as f32 / MICROSECONDS)
        .filter(|p| p.is_finite() && *p >= 0.0);
    Some(Track {
        title,
        artist,
        playing,
        position,
        duration,
        read_at: Instant::now(),
    })
}

fn text(value: &OwnedValue) -> Option<String> {
    String::try_from(value.try_clone().ok()?).ok()
}

/// Players disagree on the type of the length: signed or unsigned microseconds.
fn microseconds(value: &OwnedValue) -> Option<f32> {
    i64::try_from(value)
        .map(|v| v as f32)
        .or_else(|_| u64::try_from(value).map(|v| v as f32))
        .ok()
        .map(|v| v / MICROSECONDS)
}

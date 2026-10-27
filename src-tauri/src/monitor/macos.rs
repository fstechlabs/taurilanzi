use std::time::Instant;

use super::feeds::{Battery, Power, Track, Volume, parse_ping, safe_host};
use objc2_core_foundation::{CFDictionary, CFNumber, CFRetained, CFString, CFType};
use objc2_io_kit::{
    IOIteratorNext, IOObjectRelease, IORegistryEntryCreateCFProperty, IOServiceGetMatchingServices,
    IOServiceMatching, io_iterator_t, kIOMainPortDefault,
};

/// How busy the busiest GPU is, from the statistics its driver publishes in the IOKit registry.
pub fn gpu_utilization() -> Option<f32> {
    let matching = unsafe { IOServiceMatching(c"IOAccelerator".as_ptr()) }?;
    // A mutable dictionary is a dictionary.
    let matching = unsafe { CFRetained::cast_unchecked::<CFDictionary>(matching) };
    let mut services: io_iterator_t = 0;
    let status =
        unsafe { IOServiceGetMatchingServices(kIOMainPortDefault, Some(matching), &mut services) };
    if status != 0 || services == 0 {
        return None;
    }
    let statistics = CFString::from_static_str("PerformanceStatistics");
    let utilization = CFString::from_static_str("Device Utilization %");
    let mut busiest: Option<f32> = None;
    loop {
        let service = IOIteratorNext(services);
        if service == 0 {
            break;
        }
        let properties =
            unsafe { IORegistryEntryCreateCFProperty(service, Some(&statistics), None, 0) };
        IOObjectRelease(service);
        let percent = properties
            .and_then(|value| value.downcast::<CFDictionary>().ok())
            .and_then(|dictionary| {
                let dictionary = unsafe {
                    CFRetained::cast_unchecked::<CFDictionary<CFString, CFType>>(dictionary)
                };
                dictionary.get(&utilization)
            })
            .and_then(|value| value.downcast::<CFNumber>().ok())
            .and_then(|number| number.as_i64());
        if let Some(percent) = percent {
            let level = (percent as f32 / 100.0).clamp(0.0, 1.0);
            busiest = Some(busiest.map_or(level, |b| b.max(level)));
        }
    }
    IOObjectRelease(services);
    busiest
}

/// The internal battery, from `pmset`; none on a Mac without one.
pub fn battery() -> Option<Battery> {
    let output = std::process::Command::new("/usr/bin/pmset")
        .args(["-g", "batt"])
        .output()
        .ok()?;
    parse_pmset(&String::from_utf8_lossy(&output.stdout))
}

/// Each player is asked only while it runs: a script naming an app that is not installed would
/// make macOS ask the user where it is. The first time, macOS asks the user to let the app
/// control the player.
const PLAYERS: &[(&str, &str)] = &[
    (
        "Spotify",
        r#"tell application "Spotify"
set s to player state as text
if s is "stopped" then return ""
set t to current track
return s & tab & (name of t) & tab & (artist of t) & tab & (round (player position)) & tab & (round ((duration of t) / 1000))
end tell"#,
    ),
    (
        "Music",
        r#"tell application "Music"
set s to player state as text
if s is "stopped" then return ""
try
set t to current track
on error
return ""
end try
return s & tab & (name of t) & tab & (artist of t) & tab & (round (player position)) & tab & (round (duration of t))
end tell"#,
    ),
];

/// The song playing, or paused if none plays, in Spotify or Music.
pub fn now_playing() -> Option<Track> {
    let mut found: Vec<Track> = Vec::new();
    for (process, script) in PLAYERS {
        let running = std::process::Command::new("/usr/bin/pgrep")
            .args(["-xq", process])
            .status()
            .is_ok_and(|status| status.success());
        if !running {
            continue;
        }
        let Ok(output) = std::process::Command::new("/usr/bin/osascript")
            .args(["-e", script])
            .output()
        else {
            continue;
        };
        if let Some(track) = parse_track(&String::from_utf8_lossy(&output.stdout), Instant::now()) {
            found.push(track);
        }
    }
    let playing = found.iter().position(|t| t.playing).unwrap_or(0);
    (!found.is_empty()).then(|| found.swap_remove(playing))
}

pub fn volume() -> Option<Volume> {
    let output = std::process::Command::new("/usr/bin/osascript")
        .args(["-e", "get volume settings"])
        .output()
        .ok()?;
    parse_volume(&String::from_utf8_lossy(&output.stdout))
}

/// The router: the gateway of the default route.
pub fn router() -> Option<String> {
    let output = std::process::Command::new("/sbin/route")
        .args(["-n", "get", "default"])
        .output()
        .ok()?;
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| line.trim().strip_prefix("gateway:"))
        .map(|gateway| gateway.trim().to_owned())
        .filter(|gateway| safe_host(gateway))
}

/// Milliseconds until `host` answers, waiting a second at most.
pub fn ping(host: &str) -> Option<f32> {
    let output = std::process::Command::new("/sbin/ping")
        .args(["-n", "-c", "1", "-W", "1000", host])
        .output()
        .ok()?;
    parse_ping(&String::from_utf8_lossy(&output.stdout))
}

/// `output volume:68, input volume:88, alert volume:100, output muted:false`.
fn parse_volume(text: &str) -> Option<Volume> {
    let field = |name: &str| {
        text.split(',')
            .filter_map(|part| part.split_once(':'))
            .find(|(key, _)| key.trim() == name)
            .map(|(_, value)| value.trim().to_owned())
    };
    let level = |name: &str| {
        field(name)
            .and_then(|v| v.parse::<u8>().ok())
            .map(|v| v.min(100))
    };
    let output = level("output volume");
    let input = level("input volume");
    (output.is_some() || input.is_some()).then(|| Volume {
        output,
        input,
        muted: field("output muted").as_deref() == Some("true"),
    })
}

/// What `pmset -g batt` says, as in
/// `-InternalBattery-0 (id=1)`, a tab, then `80%; charging; 1:23 remaining present: true`.
fn parse_pmset(output: &str) -> Option<Battery> {
    let line = output.lines().find(|l| l.contains("InternalBattery"))?;
    let (_, info) = line.split_once('\t')?;
    let mut parts = info.split(';').map(str::trim);
    let percent = parts.next()?.trim_end_matches('%').trim().parse().ok()?;
    let state = parts.next().unwrap_or_default();
    let rest = parts.next().unwrap_or_default();
    let power = match state {
        "charging" | "finishing charge" => Power::Charging,
        "discharging" => Power::Draining,
        _ => Power::Plugged,
    };
    let minutes_left = rest
        .split_whitespace()
        .next()
        .and_then(|time| time.split_once(':'))
        .and_then(|(h, m)| Some(h.parse::<u32>().ok()? * 60 + m.parse::<u32>().ok()?))
        .filter(|_| power != Power::Plugged);
    Some(Battery {
        percent,
        power,
        minutes_left,
    })
}

/// A line of `state\ttitle\tartist\tposition\tduration`, as the player scripts return it.
fn parse_track(line: &str, now: Instant) -> Option<Track> {
    let mut fields = line.trim_end_matches(['\n', '\r']).split('\t');
    let state = fields.next()?.trim();
    let playing = match state {
        "playing" => true,
        "paused" => false,
        _ => return None,
    };
    let title = fields.next()?.trim().to_owned();
    let artist = fields.next().unwrap_or_default().trim().to_owned();
    let number = |text: Option<&str>| {
        text.and_then(|t| t.trim().replace(',', ".").parse::<f32>().ok())
            .filter(|n| n.is_finite() && *n >= 0.0)
    };
    let position = number(fields.next());
    let duration = number(fields.next()).filter(|d| *d > 0.0);
    (!title.is_empty()).then_some(Track {
        title,
        artist,
        playing,
        position,
        duration,
        read_at: now,
    })
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn reads_the_volume() {
        assert_eq!(
            parse_volume(
                "output volume:68, input volume:88, alert volume:100, output muted:false\n"
            ),
            Some(Volume {
                output: Some(68),
                input: Some(88),
                muted: false
            })
        );
        assert_eq!(
            parse_volume(
                "output volume:missing value, input volume:50, alert volume:100, output muted:true"
            ),
            Some(Volume {
                output: None,
                input: Some(50),
                muted: true
            })
        );
        assert_eq!(parse_volume("nonsense"), None);
    }

    #[test]
    fn reads_the_battery_from_pmset() {
        let charging = "Now drawing from 'AC Power'\n -InternalBattery-0 (id=26083427)\t62%; charging; 1:23 remaining present: true\n";
        assert_eq!(
            parse_pmset(charging),
            Some(Battery {
                percent: 62,
                power: Power::Charging,
                minutes_left: Some(83)
            })
        );
        let held = " -InternalBattery-0 (id=1)\t80%; AC attached; not charging present: true";
        assert_eq!(
            parse_pmset(held),
            Some(Battery {
                percent: 80,
                power: Power::Plugged,
                minutes_left: None
            })
        );
        let draining = " -InternalBattery-0 (id=1)\t41%; discharging; 3:05 remaining present: true";
        assert_eq!(parse_pmset(draining).unwrap().minutes_left, Some(185));
        let unknown = " -InternalBattery-0 (id=1)\t41%; discharging; (no estimate) present: true";
        assert_eq!(parse_pmset(unknown).unwrap().minutes_left, None);
        assert_eq!(
            parse_pmset("Now drawing from 'AC Power'\n"),
            None,
            "a Mac without a battery"
        );
    }

    #[test]
    fn reads_the_song_from_the_player_scripts() {
        let now = Instant::now();
        let track = parse_track("playing\tSo What\tMiles Davis\t65\t562\n", now).unwrap();
        assert_eq!(
            (track.title.as_str(), track.artist.as_str()),
            ("So What", "Miles Davis")
        );
        assert!(track.playing);
        assert_eq!((track.position, track.duration), (Some(65.0), Some(562.0)));
        let later = now + Duration::from_secs(10);
        assert_eq!(track.position_now(later), Some(75.0));

        let paused = parse_track("paused\tSong\t\t12,5\t0", now).unwrap();
        assert_eq!(
            paused.position_now(later),
            Some(12.5),
            "a paused song stays put"
        );
        assert_eq!(paused.duration, None);
        assert_eq!(parse_track("stopped\tSong\tA\t0\t1", now), None);
        assert_eq!(parse_track("", now), None);
    }
}

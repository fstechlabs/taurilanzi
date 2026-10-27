//! PipeWire's `wpctl`, which current desktops have; PulseAudio's `pactl` on older ones.

use std::process::Command;

use crate::monitor::feeds::Volume;

pub fn volume() -> Option<Volume> {
    let (output, muted) = match wpctl("@DEFAULT_AUDIO_SINK@") {
        Some(reading) => (Some(reading.0), reading.1),
        None => match pactl_sink() {
            Some(reading) => (Some(reading.0), reading.1),
            None => (None, false),
        },
    };
    let input = wpctl("@DEFAULT_AUDIO_SOURCE@")
        .or_else(pactl_source)
        .map(|(level, _)| level);
    (output.is_some() || input.is_some()).then_some(Volume {
        output,
        input,
        muted,
    })
}

fn run(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

fn wpctl(node: &str) -> Option<(u8, bool)> {
    parse_wpctl(&run("wpctl", &["get-volume", node])?)
}

fn pactl_sink() -> Option<(u8, bool)> {
    let level = parse_pactl_volume(&run("pactl", &["get-sink-volume", "@DEFAULT_SINK@"])?)?;
    let muted =
        run("pactl", &["get-sink-mute", "@DEFAULT_SINK@"]).is_some_and(|text| text.contains("yes"));
    Some((level, muted))
}

fn pactl_source() -> Option<(u8, bool)> {
    let level = parse_pactl_volume(&run("pactl", &["get-source-volume", "@DEFAULT_SOURCE@"])?)?;
    Some((level, false))
}

/// `Volume: 0.45` or `Volume: 0.45 [MUTED]`.
fn parse_wpctl(text: &str) -> Option<(u8, bool)> {
    let value: f32 = text
        .trim()
        .strip_prefix("Volume:")?
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    let level = (value * 100.0).round().clamp(0.0, 100.0) as u8;
    Some((level, text.contains("[MUTED]")))
}

/// `Volume: front-left: 29491 /  45% / -20.81 dB,   front-right: …`: the first channel's percent.
fn parse_pactl_volume(text: &str) -> Option<u8> {
    text.split('/')
        .find_map(|part| part.trim().strip_suffix('%')?.trim().parse::<u16>().ok())
        .map(|percent| percent.min(100) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_volume_tools() {
        assert_eq!(parse_wpctl("Volume: 0.45\n"), Some((45, false)));
        assert_eq!(parse_wpctl("Volume: 1.20 [MUTED]\n"), Some((100, true)));
        assert_eq!(parse_wpctl("nonsense"), None);
        assert_eq!(
            parse_pactl_volume(
                "Volume: front-left: 29491 /  45% / -20.81 dB,   front-right: 29491 /  45% / -20.81 dB\n"
            ),
            Some(45)
        );
        assert_eq!(parse_pactl_volume("Volume:"), None);
    }
}

//! The song any app plays, from the media session Windows shows over the volume flyout.

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSessionManager as Sessions,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Playback,
};

use crate::monitor::feeds::Track;
use crate::os::windows::multithreaded;

/// Windows counts time in 100 ns steps from 1601.
const TICKS_PER_SECOND: f64 = 10_000_000.0;
const SECONDS_FROM_1601_TO_1970: u64 = 11_644_473_600;

pub fn now_playing() -> Option<Track> {
    multithreaded();
    let session = Sessions::RequestAsync()
        .ok()?
        .join()
        .ok()?
        .GetCurrentSession()
        .ok()?;
    let playing = match session.GetPlaybackInfo().ok()?.PlaybackStatus().ok()? {
        Playback::Playing => true,
        Playback::Paused => false,
        _ => return None,
    };
    let properties = session.TryGetMediaPropertiesAsync().ok()?.join().ok()?;
    let title = properties.Title().ok()?.to_string_lossy();
    if title.is_empty() {
        return None;
    }
    let artist = properties
        .Artist()
        .map(|a| a.to_string_lossy())
        .unwrap_or_default();
    let timeline = session.GetTimelineProperties().ok();
    let seconds = |ticks: i64| (ticks as f64 / TICKS_PER_SECOND) as f32;
    let duration = timeline
        .as_ref()
        .and_then(|t| t.EndTime().ok())
        .map(|end| seconds(end.Duration))
        .filter(|d| *d > 0.0);
    // The player updates the position now and then; while playing it moved on since.
    let position = timeline.as_ref().and_then(|t| {
        let at = seconds(t.Position().ok()?.Duration);
        let updated = t.LastUpdatedTime().ok()?.UniversalTime;
        let since = if playing {
            (now_ticks() - updated).max(0)
        } else {
            0
        };
        let position = at + seconds(since);
        Some(duration.map_or(position, |d| position.min(d)))
    });
    Some(Track {
        title,
        artist,
        playing,
        position: position.filter(|p| p.is_finite() && *p >= 0.0),
        duration,
        read_at: Instant::now(),
    })
}

fn now_ticks() -> i64 {
    let since_1970 = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();
    ((since_1970 + SECONDS_FROM_1601_TO_1970 as f64) * TICKS_PER_SECOND) as i64
}

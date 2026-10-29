//! Works out what a widget shows right now, for the renderer to draw.

use jiff::civil::DateTime;
use jiff::tz::TimeZone;
use jiff::{Timestamp, Zoned};

use std::time::{Duration, Instant};

use super::hass_widgets;
use super::key_widgets::{KeyNow, StopwatchNow, TimerNow};
use crate::config::{AssetStore, ClockLine, Metric, Period, Usage, Widget, WorldClock};
use std::collections::BTreeMap;

use uuid::Uuid;

use crate::hass::HassReport;
use crate::monitor::{
    Battery, Monitor, Pings, Power, Processes, Readings, SAMPLE_EVERY, Temperatures, Track, Volume,
};
use crate::render::{ClockFace, Meter, NetworkGraph, Rate, TimerPhase, WidgetContent, WidgetView};

const MISSING: &str = "—";
const METRICS_MAX: usize = 4;
/// Bytes per second.
const PEAK_FLOOR: f64 = 1000.0;
/// Enough to draw a smooth line on the wide key without a heavy image.
const GRAPH_POINTS: usize = 120;
const GRAPH_MINUTES: (u32, u32) = (1, 24 * 60);
const WORLD_CLOCKS_MAX: usize = 3;
const LOW_BATTERY: u8 = 20;

/// Where widgets read what they show.
pub struct Feeds<'a> {
    pub monitor: &'a Monitor,
    pub hass: &'a BTreeMap<Uuid, HassReport>,
    pub assets: &'a AssetStore,
}

/// What a widget shows now. `key` is the state of widgets that answer to their key.
pub fn view(widget: &Widget, feeds: &Feeds, key: KeyNow, now: Timestamp) -> WidgetView {
    let style = widget.style();
    let monitor = feeds.monitor;
    let content = match widget {
        Widget::HassEntity { .. }
        | Widget::HassGraph { .. }
        | Widget::HassEntities { .. }
        | Widget::HassPicture { .. }
        | Widget::HassWeather { .. }
        | Widget::HassTemplate { .. }
        | Widget::HassTile { .. } => hass_widgets::view(widget, feeds.hass, feeds.assets, now),
        Widget::DigitalClock {
            seconds,
            hour12,
            second_line,
            zone,
            label,
            ..
        } => {
            let time = zoned(now, zone.as_deref());
            let (hour, suffix) = if *hour12 {
                let suffix = if time.hour() < 12 { "AM" } else { "PM" };
                (
                    format!("{}", (time.hour() + 11) % 12 + 1),
                    Some(suffix.into()),
                )
            } else {
                (format!("{:02}", time.hour()), None)
            };
            WidgetContent::DigitalClock {
                time: format!("{hour}:{:02}", time.minute()),
                seconds: seconds.then(|| format!("{:02}", time.second())),
                suffix,
                label: text(label),
                line: match second_line {
                    ClockLine::None => None,
                    ClockLine::Date => {
                        let (date, year) = (day_month(&time), time.year());
                        Some(trf!("widgets.date.withYear", date = date, year = year))
                    }
                    ClockLine::Weekday => Some(weekday(&time).into()),
                    ClockLine::WeekdayDate => {
                        Some(format!("{}, {}", weekday(&time), day_month(&time)))
                    }
                },
            }
        }
        Widget::AnalogClock {
            seconds,
            date,
            zone,
            label,
            ..
        } => {
            let time = zoned(now, zone.as_deref());
            let second = if *seconds {
                f32::from(time.second())
            } else {
                0.0
            };
            let minute = f32::from(time.minute()) + second / 60.0;
            let hour = f32::from(time.hour() % 12) + minute / 60.0;
            WidgetContent::AnalogClock {
                hour: hour * 30.0,
                minute: minute * 6.0,
                second: seconds.then_some(second * 6.0),
                label: text(label),
                weekday: date.then(|| weekday(&time).into()),
                date: date.then(|| day_month(&time)),
            }
        }
        Widget::SystemMonitor {
            metrics,
            graph,
            minutes,
            ..
        } => {
            let readings = monitor.readings();
            let history = if *graph {
                window(monitor.history(), *minutes)
            } else {
                Vec::new()
            };
            WidgetContent::Meters {
                meters: metrics
                    .iter()
                    .take(METRICS_MAX)
                    .map(|metric| meter(*metric, &readings, &history))
                    .collect(),
                graph: *graph,
            }
        }
        Widget::Network { graph, minutes, .. } => {
            let readings = monitor.readings();
            WidgetContent::Network {
                down: rate(readings.down),
                up: rate(readings.up),
                graph: graph.then(|| network_graph(&window(monitor.history(), *minutes))),
            }
        }
        Widget::Countdown {
            target,
            label,
            done,
            ..
        } => {
            let (main, detail) = match left_until(target, now) {
                None => (no_date().into(), None),
                Some(left) if left <= 0 => (text(done).unwrap_or_else(|| done_text().into()), None),
                Some(left) => {
                    let days = left / 86_400;
                    let clock = format!(
                        "{:02}:{:02}:{:02}",
                        left % 86_400 / 3600,
                        left % 3600 / 60,
                        left % 60
                    );
                    match days {
                        0 => (clock, None),
                        1 => (tr!("widgets.countdown.oneDay").into(), Some(clock)),
                        _ => (trf!("widgets.countdown.days", days = days), Some(clock)),
                    }
                }
            };
            WidgetContent::Countdown {
                label: text(label),
                main,
                detail,
            }
        }
        Widget::WorldClocks { clocks, hour12, .. } => WidgetContent::WorldClocks {
            clocks: clocks
                .iter()
                .take(WORLD_CLOCKS_MAX)
                .map(|clock| face(clock, *hour12, now, &TimeZone::system()))
                .collect(),
        },
        Widget::Calendar { .. } => calendar(&now.to_zoned(TimeZone::system())),
        Widget::Timer {
            seconds,
            label,
            done,
            ..
        } => timer_content(*seconds, text(label), done, key.timer),
        Widget::Battery { .. } => battery(monitor.battery()),
        Widget::NowPlaying { .. } => now_playing(monitor.track(), Instant::now()),
        Widget::Temperatures { .. } => temperatures(monitor.temperatures()),
        Widget::TopProcesses { by, .. } => top_processes(monitor.processes(), *by),
        Widget::Latency { host, .. } => latency(host, monitor.pings(host)),
        Widget::Volume { .. } => volume(monitor.volume()),
        Widget::Moon { .. } => moon(now),
        Widget::Progress { periods, .. } => progress(periods, &now.to_zoned(TimeZone::system())),
        Widget::Stopwatch { label, .. } => stopwatch(text(label), key.stopwatch),
        Widget::Counter { label, .. } => WidgetContent::Countdown {
            label: text(label),
            main: key.count.to_string(),
            detail: None,
        },
        Widget::WordClock { .. } => word_clock(&now.to_zoned(TimeZone::system())),
        Widget::Phrases {
            phrases, seconds, ..
        } => phrase(phrases, *seconds, key.presses, now),
    };
    WidgetView {
        color: style.color,
        accent: style.accent,
        font_family: style.font_family.clone(),
        content,
    }
}

/// The time in `zone`, or in the Mac's own when it has none or the name is unknown.
fn zoned(now: Timestamp, zone: Option<&str>) -> Zoned {
    let zone = zone
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .and_then(|name| TimeZone::get(name).ok())
        .unwrap_or_else(TimeZone::system);
    now.to_zoned(zone)
}

fn done_text() -> &'static str {
    tr!("widgets.countdown.done")
}

fn no_date() -> &'static str {
    tr!("widgets.countdown.noDate")
}

fn timer_done() -> &'static str {
    tr!("widgets.timer.done")
}

pub(super) fn weekday(time: &Zoned) -> &'static str {
    match time.weekday().to_monday_zero_offset() {
        0 => tr!("widgets.weekdays.monday"),
        1 => tr!("widgets.weekdays.tuesday"),
        2 => tr!("widgets.weekdays.wednesday"),
        3 => tr!("widgets.weekdays.thursday"),
        4 => tr!("widgets.weekdays.friday"),
        5 => tr!("widgets.weekdays.saturday"),
        _ => tr!("widgets.weekdays.sunday"),
    }
}

fn month_name(time: &Zoned) -> &'static str {
    match time.month() {
        1 => tr!("widgets.months.january"),
        2 => tr!("widgets.months.february"),
        3 => tr!("widgets.months.march"),
        4 => tr!("widgets.months.april"),
        5 => tr!("widgets.months.may"),
        6 => tr!("widgets.months.june"),
        7 => tr!("widgets.months.july"),
        8 => tr!("widgets.months.august"),
        9 => tr!("widgets.months.september"),
        10 => tr!("widgets.months.october"),
        11 => tr!("widgets.months.november"),
        _ => tr!("widgets.months.december"),
    }
}

/// `4 de marzo`, `March 4`.
pub(super) fn day_month(time: &Zoned) -> String {
    let (day, month) = (time.day(), month_name(time));
    trf!("widgets.date.dayMonth", day = day, month = month)
}

/// `1.5`, or `1,5` where the language writes a decimal comma.
pub(super) fn decimal(text: String) -> String {
    text.replace('.', tr!("widgets.decimalSeparator"))
}

pub(super) fn text(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

/// Whole seconds left until `target`, a local date and time, rounded up so it reads zero just
/// as it arrives. None when the date cannot be read.
fn left_until(target: &str, now: Timestamp) -> Option<i64> {
    let target = target.trim().parse::<DateTime>().ok()?;
    let at = target.to_zoned(TimeZone::system()).ok()?.timestamp();
    let millis = at.as_millisecond() - now.as_millisecond();
    Some(millis.div_euclid(1000) + i64::from(millis.rem_euclid(1000) > 0))
}

/// `here` is the Mac's time zone, to tell when the day there is another.
fn face(clock: &WorldClock, hour12: bool, now: Timestamp, here: &TimeZone) -> ClockFace {
    let there = zoned(now, Some(&clock.zone));
    let here = now.to_zoned(here.clone()).date();
    let (time, suffix) = clock_time(&there, hour12);
    let city = clock
        .zone
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .replace('_', " ");
    ClockFace {
        label: text(&clock.label).unwrap_or(city),
        time,
        suffix,
        day: match there.date().cmp(&here) {
            std::cmp::Ordering::Greater => Some(tr!("widgets.worldClocks.tomorrow").into()),
            std::cmp::Ordering::Less => Some(tr!("widgets.worldClocks.yesterday").into()),
            std::cmp::Ordering::Equal => None,
        },
        night: !(7..20).contains(&there.hour()),
    }
}

fn clock_time(time: &Zoned, hour12: bool) -> (String, Option<String>) {
    if hour12 {
        let suffix = if time.hour() < 12 { "AM" } else { "PM" };
        (
            format!("{}:{:02}", (time.hour() + 11) % 12 + 1, time.minute()),
            Some(suffix.into()),
        )
    } else {
        (format!("{:02}:{:02}", time.hour(), time.minute()), None)
    }
}

fn calendar(today: &Zoned) -> WidgetContent {
    let first = today.date().first_of_month();
    let offset = first.weekday().to_monday_zero_offset() as usize;
    let days = today.date().days_in_month() as usize;
    let mut weeks = Vec::new();
    let mut week = [0u8; 7];
    for cell in 0..(offset + days).div_ceil(7) * 7 {
        if cell >= offset && cell < offset + days {
            week[cell % 7] = (cell - offset + 1) as u8;
        }
        if cell % 7 == 6 {
            weeks.push(week);
            week = [0; 7];
        }
    }
    let month = month_name(today);
    let mut chars = month.chars();
    let month = chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect::<String>())
        .unwrap_or_default();
    WidgetContent::Calendar {
        weekday: weekday(today).into(),
        day: today.day().to_string(),
        month: format!("{month} {}", today.year()),
        weeks,
        today: today.day() as u8,
    }
}

/// `25:00`, or `1:05:00` from an hour up. Seconds round up, so it reads 0 only at the end.
fn clock_left(left: Duration) -> String {
    let total = left.as_secs() + u64::from(left.subsec_nanos() > 0);
    if total >= 3600 {
        format!(
            "{}:{:02}:{:02}",
            total / 3600,
            total % 3600 / 60,
            total % 60
        )
    } else {
        format!("{:02}:{:02}", total / 60, total % 60)
    }
}

fn timer_content(
    seconds: u32,
    label: Option<String>,
    done: &str,
    timer: TimerNow,
) -> WidgetContent {
    let length = Duration::from_secs(u64::from(seconds.max(1)));
    let gone = |left: Duration| 1.0 - left.as_secs_f32() / length.as_secs_f32();
    let (main, progress, phase) = match timer {
        TimerNow::Idle => (clock_left(length), 0.0, TimerPhase::Idle),
        TimerNow::Running(left) => (clock_left(left), gone(left), TimerPhase::Running),
        TimerNow::Paused(left) => (clock_left(left), gone(left), TimerPhase::Paused),
        TimerNow::Done => (
            text(done).unwrap_or_else(|| timer_done().into()),
            1.0,
            TimerPhase::Done,
        ),
    };
    WidgetContent::Timer {
        label,
        main,
        progress: progress.clamp(0.0, 1.0),
        phase,
    }
}

fn hours_minutes(minutes: u32) -> String {
    format!("{}:{:02}", minutes / 60, minutes % 60)
}

fn battery(reading: Option<Option<Battery>>) -> WidgetContent {
    let Some(Some(battery)) = reading else {
        return WidgetContent::Battery {
            level: None,
            text: MISSING.into(),
            detail: if reading.is_some() {
                tr!("widgets.battery.none")
            } else {
                ""
            }
            .into(),
            charging: false,
            low: false,
        };
    };
    let detail = match (battery.power, battery.minutes_left) {
        (Power::Charging, Some(left)) => {
            trf!("widgets.battery.chargingFullIn", time = hours_minutes(left))
        }
        (Power::Charging, None) => tr!("widgets.battery.charging").into(),
        (Power::Plugged, _) if battery.percent >= 100 => tr!("widgets.battery.charged").into(),
        (Power::Plugged, _) => tr!("widgets.battery.pluggedIn").into(),
        (Power::Draining, Some(left)) => {
            trf!("widgets.battery.timeLeft", time = hours_minutes(left))
        }
        (Power::Draining, None) => tr!("widgets.battery.onBattery").into(),
    };
    WidgetContent::Battery {
        level: Some(f32::from(battery.percent.min(100)) / 100.0),
        text: format!("{} %", battery.percent),
        detail,
        charging: battery.power == Power::Charging,
        low: battery.power == Power::Draining && battery.percent <= LOW_BATTERY,
    }
}

/// `3:42`, or `1:02:03` for long tracks.
pub(super) fn track_time(seconds: f32) -> String {
    let total = seconds.max(0.0) as u64;
    if total >= 3600 {
        format!(
            "{}:{:02}:{:02}",
            total / 3600,
            total % 3600 / 60,
            total % 60
        )
    } else {
        format!("{}:{:02}", total / 60, total % 60)
    }
}

fn now_playing(reading: Option<Option<Track>>, now: Instant) -> WidgetContent {
    let Some(Some(track)) = reading else {
        return WidgetContent::NowPlaying {
            title: None,
            artist: if reading.is_some() {
                tr!("widgets.nowPlaying.nothing")
            } else {
                ""
            }
            .into(),
            playing: false,
            progress: None,
            time: None,
        };
    };
    let position = track.position_now(now);
    WidgetContent::NowPlaying {
        title: Some(track.title.clone()),
        artist: track.artist.clone(),
        playing: track.playing,
        progress: position
            .zip(track.duration)
            .map(|(p, d)| (p / d).clamp(0.0, 1.0)),
        time: position.map(|p| match track.duration {
            Some(d) => format!("{} / {}", track_time(p), track_time(d)),
            None => track_time(p),
        }),
    }
}

const PERIODS_MAX: usize = 4;
const PROCESS_NAME_MAX: usize = 14;
/// Degrees shown as an empty bar and as a full one.
const TEMPERATURE_RANGE: (f32, f32) = (20.0, 100.0);
/// Days from new moon to new moon.
const SYNODIC_MONTH: f64 = 29.530_588_853;
/// A new moon: 6 January 2000, 18:14 UTC.
const NEW_MOON_SECONDS: i64 = 947_182_440;
fn reading_text() -> &'static str {
    tr!("widgets.reading")
}

/// A row of bars when there is nothing to show yet.
fn waiting(label: &str) -> WidgetContent {
    WidgetContent::Meters {
        meters: vec![Meter {
            label: label.into(),
            level: None,
            text: String::new(),
            history: Vec::new(),
        }],
        graph: false,
    }
}

fn level_meter(label: &str, level: Option<f32>, text: String) -> Meter {
    Meter {
        label: label.into(),
        level,
        text,
        history: Vec::new(),
    }
}

fn temperatures(reading: Option<Option<Temperatures>>) -> WidgetContent {
    let Some(Some(reading)) = reading else {
        return waiting(if reading.is_some() {
            tr!("widgets.temperatures.noSensors")
        } else {
            reading_text()
        });
    };
    let row = |label: &str, value: Option<f32>| {
        let (low, high) = TEMPERATURE_RANGE;
        level_meter(
            label,
            value.map(|t| ((t - low) / (high - low)).clamp(0.0, 1.0)),
            value.map_or_else(|| MISSING.into(), |t| format!("{t:.0} °C")),
        )
    };
    WidgetContent::Meters {
        meters: vec![
            row("CPU", reading.cpu),
            row(tr!("widgets.temperatures.disk"), reading.disk),
            row(tr!("widgets.temperatures.battery"), reading.battery),
        ]
        .into_iter()
        .filter(|m| m.level.is_some())
        .collect::<Vec<_>>()
        .into_iter()
        .chain(
            (reading.cpu.is_none() && reading.disk.is_none() && reading.battery.is_none())
                .then(|| row("CPU", None)),
        )
        .collect(),
        graph: false,
    }
}

/// `Google Chrome Helper` fits as `Google Chrome…`.
fn short_name(name: &str) -> String {
    if name.chars().count() <= PROCESS_NAME_MAX {
        name.to_owned()
    } else {
        let cut: String = name.chars().take(PROCESS_NAME_MAX - 1).collect();
        format!("{}…", cut.trim_end())
    }
}

fn top_processes(reading: Option<Option<Processes>>, by: Usage) -> WidgetContent {
    let Some(Some(processes)) = reading else {
        return waiting(reading_text());
    };
    let list = match by {
        Usage::Cpu => &processes.by_cpu,
        Usage::Memory => &processes.by_memory,
    };
    WidgetContent::Meters {
        meters: list
            .iter()
            .take(3)
            .map(|p| {
                let percent = p.share * 100.0;
                let text = if percent < 10.0 {
                    decimal(format!("{percent:.1} %"))
                } else {
                    format!("{percent:.0} %")
                };
                level_meter(&short_name(&p.name), Some(p.share), text)
            })
            .collect(),
        graph: false,
    }
}

fn milliseconds(ms: f32) -> String {
    if ms < 10.0 {
        decimal(format!("{ms:.1} ms"))
    } else {
        format!("{ms:.0} ms")
    }
}

fn latency(host: &str, reading: Option<Option<Pings>>) -> WidgetContent {
    let host = host.trim();
    let target = |found: Option<&str>| match (host.is_empty(), found) {
        (true, Some(router)) => format!("Router · {router}"),
        (true, None) => "Router".into(),
        (false, _) => host.into(),
    };
    let Some(Some(pings)) = reading else {
        return WidgetContent::Latency {
            value: MISSING.into(),
            target: match reading {
                Some(None) if host.is_empty() => tr!("widgets.latency.noRouter").into(),
                Some(None) => trf!("widgets.latency.invalidHost", host = host),
                _ => target(None),
            },
            history: Vec::new(),
            peak: String::new(),
            ok: false,
        };
    };
    let last = pings.history.last().copied().flatten();
    let peak = pings
        .history
        .iter()
        .flatten()
        .copied()
        .fold(10.0_f32, f32::max)
        * 1.2;
    let missing = PING_HISTORY_LEN.saturating_sub(pings.history.len());
    WidgetContent::Latency {
        value: last.map_or_else(|| tr!("widgets.latency.noReply").into(), milliseconds),
        target: target(Some(&pings.target)),
        history: std::iter::repeat_n(None, missing)
            .chain(
                pings
                    .history
                    .iter()
                    .map(|ms| ms.map(|ms| (ms / peak).clamp(0.0, 1.0))),
            )
            .collect(),
        peak: milliseconds(peak),
        ok: last.is_some(),
    }
}

const PING_HISTORY_LEN: usize = crate::monitor::PING_HISTORY;

fn volume(reading: Option<Option<Volume>>) -> WidgetContent {
    let Some(Some(volume)) = reading else {
        return waiting(reading_text());
    };
    let level = |v: Option<u8>| v.map(|v| f32::from(v) / 100.0);
    let percent = |v: Option<u8>| v.map_or_else(|| MISSING.into(), |v| format!("{v} %"));
    let speakers_label = tr!("widgets.volume.speakers");
    let speakers = if volume.muted {
        level_meter(
            speakers_label,
            Some(0.0),
            tr!("widgets.volume.muted").into(),
        )
    } else {
        level_meter(speakers_label, level(volume.output), percent(volume.output))
    };
    WidgetContent::Meters {
        meters: vec![
            speakers,
            level_meter(
                tr!("widgets.volume.mic"),
                level(volume.input),
                percent(volume.input),
            ),
        ],
        graph: false,
    }
}

fn moon(now: Timestamp) -> WidgetContent {
    let days = (now.as_second() - NEW_MOON_SECONDS) as f64 / 86_400.0;
    let age = days.rem_euclid(SYNODIC_MONTH);
    let phase = (age / SYNODIC_MONTH) as f32;
    let name = match age / SYNODIC_MONTH * 8.0 {
        x if !(0.5..7.5).contains(&x) => tr!("widgets.moon.phases.new"),
        x if x < 1.5 => tr!("widgets.moon.phases.waxingCrescent"),
        x if x < 2.5 => tr!("widgets.moon.phases.firstQuarter"),
        x if x < 3.5 => tr!("widgets.moon.phases.waxingGibbous"),
        x if x < 4.5 => tr!("widgets.moon.phases.full"),
        x if x < 5.5 => tr!("widgets.moon.phases.waningGibbous"),
        x if x < 6.5 => tr!("widgets.moon.phases.lastQuarter"),
        _ => tr!("widgets.moon.phases.waningCrescent"),
    };
    let lit = (1.0 - (2.0 * std::f64::consts::PI * age / SYNODIC_MONTH).cos()) / 2.0;
    let (next, wait) = if age < SYNODIC_MONTH / 2.0 {
        (tr!("widgets.moon.next.full"), SYNODIC_MONTH / 2.0 - age)
    } else {
        (tr!("widgets.moon.next.new"), SYNODIC_MONTH - age)
    };
    let wait = wait.round() as i64;
    let when = match wait {
        0 => tr!("widgets.moon.today").into(),
        1 => tr!("widgets.moon.tomorrow").into(),
        n => trf!("widgets.moon.inDays", days = n),
    };
    WidgetContent::Moon {
        phase,
        name: name.into(),
        detail: format!("{:.0} % · {next} {when}", lit * 100.0),
    }
}

fn progress(periods: &[Period], now: &Zoned) -> WidgetContent {
    let day =
        (f64::from(now.hour()) * 3600.0 + f64::from(now.minute()) * 60.0 + f64::from(now.second()))
            / 86_400.0;
    WidgetContent::Meters {
        meters: periods
            .iter()
            .take(PERIODS_MAX)
            .map(|period| {
                let (label, gone) = match period {
                    Period::Day => (tr!("widgets.progress.day"), day),
                    Period::Week => (
                        tr!("widgets.progress.week"),
                        (f64::from(now.weekday().to_monday_zero_offset()) + day) / 7.0,
                    ),
                    Period::Month => (
                        tr!("widgets.progress.month"),
                        (f64::from(now.day() - 1) + day) / f64::from(now.date().days_in_month()),
                    ),
                    Period::Year => (
                        tr!("widgets.progress.year"),
                        (f64::from(now.day_of_year() - 1) + day)
                            / f64::from(now.date().days_in_year()),
                    ),
                };
                let gone = gone.clamp(0.0, 1.0) as f32;
                level_meter(
                    label,
                    Some(gone),
                    format!("{:.0} %", (gone * 100.0).floor()),
                )
            })
            .collect(),
        graph: false,
    }
}

fn stopwatch(label: Option<String>, watch: StopwatchNow) -> WidgetContent {
    let total = watch.elapsed.as_secs();
    let main = if total >= 3600 {
        format!(
            "{}:{:02}:{:02}",
            total / 3600,
            total % 3600 / 60,
            total % 60
        )
    } else {
        format!("{:02}:{:02}", total / 60, total % 60)
    };
    let detail = match (watch.running, watch.elapsed.is_zero()) {
        (true, _) => None,
        (false, true) => Some(tr!("widgets.stopwatch.pressToStart").into()),
        (false, false) => Some(tr!("widgets.stopwatch.paused").into()),
    };
    WidgetContent::Countdown {
        label,
        main,
        detail,
    }
}

/// "Son las tres y cuarto", "It's quarter past three", to the nearest five minutes.
fn word_clock(now: &Zoned) -> WidgetContent {
    let minutes = (i32::from(now.minute()) + 2) / 5 * 5;
    let hour = (i32::from(now.hour()) + i32::from(minutes > 30)).rem_euclid(12);
    let phrase = match minutes % 60 {
        0 => tr!("widgets.wordClock.minutes.oClock"),
        5 => tr!("widgets.wordClock.minutes.fivePast"),
        10 => tr!("widgets.wordClock.minutes.tenPast"),
        15 => tr!("widgets.wordClock.minutes.quarterPast"),
        20 => tr!("widgets.wordClock.minutes.twentyPast"),
        25 => tr!("widgets.wordClock.minutes.twentyFivePast"),
        30 => tr!("widgets.wordClock.minutes.halfPast"),
        35 => tr!("widgets.wordClock.minutes.twentyFiveTo"),
        40 => tr!("widgets.wordClock.minutes.twentyTo"),
        45 => tr!("widgets.wordClock.minutes.quarterTo"),
        50 => tr!("widgets.wordClock.minutes.tenTo"),
        _ => tr!("widgets.wordClock.minutes.fiveTo"),
    };
    let lead = if hour == 1 {
        tr!("widgets.wordClock.leadOne")
    } else {
        tr!("widgets.wordClock.lead")
    };
    // The hour is drawn on its own, so the phrase is split around it: English names the minutes
    // before the hour, Spanish after it.
    let (before, after) = phrase.split_once("{hour}").unwrap_or((phrase, ""));
    WidgetContent::WordClock {
        lead: format!("{lead} {}", before.trim()).trim_end().to_owned(),
        hour: hour_word(hour).into(),
        rest: after.trim().into(),
    }
}

fn hour_word(hour: i32) -> &'static str {
    match hour {
        1 => tr!("widgets.wordClock.hours.one"),
        2 => tr!("widgets.wordClock.hours.two"),
        3 => tr!("widgets.wordClock.hours.three"),
        4 => tr!("widgets.wordClock.hours.four"),
        5 => tr!("widgets.wordClock.hours.five"),
        6 => tr!("widgets.wordClock.hours.six"),
        7 => tr!("widgets.wordClock.hours.seven"),
        8 => tr!("widgets.wordClock.hours.eight"),
        9 => tr!("widgets.wordClock.hours.nine"),
        10 => tr!("widgets.wordClock.hours.ten"),
        11 => tr!("widgets.wordClock.hours.eleven"),
        _ => tr!("widgets.wordClock.hours.twelve"),
    }
}

fn phrase(phrases: &[String], seconds: u32, presses: u32, now: Timestamp) -> WidgetContent {
    let list: Vec<&str> = phrases
        .iter()
        .map(|p| p.trim())
        .filter(|p| !p.is_empty())
        .collect();
    if list.is_empty() {
        return WidgetContent::Phrase {
            text: tr!("widgets.phrases.empty").into(),
        };
    }
    let turns = if seconds == 0 {
        0
    } else {
        (now.as_second().max(0) as u64) / u64::from(seconds)
    };
    let index = (turns + u64::from(presses)) % list.len() as u64;
    WidgetContent::Phrase {
        text: list[index as usize].into(),
    }
}

fn meter(metric: Metric, readings: &Readings, history: &[Readings]) -> Meter {
    let level = |readings: &Readings| match metric {
        Metric::Cpu => readings.cpu,
        Metric::Ram => readings.ram,
        Metric::Gpu => readings.gpu,
        Metric::Disk => readings.disk,
    };
    let label = match metric {
        Metric::Cpu => "CPU",
        Metric::Ram => "RAM",
        Metric::Gpu => "GPU",
        Metric::Disk => tr!("widgets.systemMonitor.disk"),
    };
    let now = level(readings);
    Meter {
        label: label.into(),
        level: now,
        text: now.map_or_else(|| MISSING.into(), |level| format!("{:.0} %", level * 100.0)),
        history: history.iter().map(level).collect(),
    }
}

/// The readings of the last `minutes`, missing ones first so a graph that just started fills
/// from the right, averaged down to at most `GRAPH_POINTS`.
fn window(history: Vec<Readings>, minutes: u32) -> Vec<Readings> {
    let minutes = minutes.clamp(GRAPH_MINUTES.0, GRAPH_MINUTES.1) as usize;
    let size = minutes * 60 / SAMPLE_EVERY.as_secs() as usize;
    let skip = history.len().saturating_sub(size);
    let missing = size.saturating_sub(history.len());
    let full: Vec<Readings> = std::iter::repeat_n(Readings::default(), missing)
        .chain(history.into_iter().skip(skip))
        .collect();
    let bucket = full.len().div_ceil(GRAPH_POINTS).max(1);
    full.chunks(bucket).map(average).collect()
}

fn average(readings: &[Readings]) -> Readings {
    fn mean<T: Into<f64> + Copy>(values: impl Iterator<Item = Option<T>>) -> Option<f64> {
        let (sum, count) = values
            .flatten()
            .fold((0.0, 0), |(sum, count), v| (sum + v.into(), count + 1));
        (count > 0).then(|| sum / f64::from(count))
    }
    let level =
        |pick: fn(&Readings) -> Option<f32>| mean(readings.iter().map(pick)).map(|v| v as f32);
    Readings {
        cpu: level(|r| r.cpu),
        ram: level(|r| r.ram),
        gpu: level(|r| r.gpu),
        disk: level(|r| r.disk),
        down: mean(readings.iter().map(|r| r.down)),
        up: mean(readings.iter().map(|r| r.up)),
    }
}

/// Both speeds against the fastest one in the history, so small traffic still shows. The top is
/// never below 1 KB/s, or an idle connection would look busy.
fn network_graph(history: &[Readings]) -> NetworkGraph {
    let peak = history
        .iter()
        .flat_map(|r| [r.down, r.up])
        .flatten()
        .fold(PEAK_FLOOR, f64::max);
    let scaled = |speed: Option<f64>| speed.map(|s| (s / peak) as f32);
    NetworkGraph {
        down: history.iter().map(|r| scaled(r.down)).collect(),
        up: history.iter().map(|r| scaled(r.up)).collect(),
        peak: rate(Some(peak)),
    }
}

/// Decimal units, as the Mac shows speeds, with one decimal below ten.
fn rate(bytes_per_second: Option<f64>) -> Rate {
    let Some(bytes) = bytes_per_second else {
        return Rate {
            value: MISSING.into(),
            unit: String::new(),
        };
    };
    let (value, unit) = [(1e9, "GB/s"), (1e6, "MB/s"), (1e3, "KB/s")]
        .into_iter()
        .find(|(scale, _)| bytes >= *scale)
        .map_or((bytes, "B/s"), |(scale, unit)| (bytes / scale, unit));
    let value = if value < 10.0 && unit != "B/s" {
        decimal(format!("{value:.1}"))
    } else {
        format!("{value:.0}")
    };
    Rate {
        value,
        unit: unit.into(),
    }
}

#[cfg(test)]
mod tests {
    use serde::de::DeserializeOwned;
    use serde_json::json;

    use super::*;

    fn show(widget: &Widget, key: KeyNow, now: Timestamp) -> WidgetContent {
        let dir = tempfile::tempdir().unwrap();
        let assets = AssetStore::open(dir.path().to_path_buf()).unwrap();
        let feeds = Feeds {
            monitor: &Monitor::default(),
            hass: &Default::default(),
            assets: &assets,
        };
        view(widget, &feeds, key, now).content
    }
    use crate::i18n::{self, Language};

    fn style<T: DeserializeOwned>() -> T {
        serde_json::from_value(json!({
            "color": { "r": 255, "g": 255, "b": 255 },
            "accent": { "r": 0, "g": 0, "b": 0 },
            "font_family": "Inter",
        }))
        .unwrap()
    }

    /// Wednesday, 4 March 2026, 13:05:09.250 UTC.
    fn now() -> Timestamp {
        "2026-03-04T13:05:09.25Z".parse().unwrap()
    }

    fn digital(hour12: bool, second_line: ClockLine, zone: Option<&str>) -> WidgetContent {
        let widget = Widget::DigitalClock {
            seconds: true,
            hour12,
            second_line,
            zone: zone.map(Into::into),
            label: "  Madrid ".into(),
            style: style(),
        };
        show(&widget, KeyNow::default(), now())
    }

    fn countdown(target: &str, done: &str, now: Timestamp) -> (String, Option<String>) {
        let widget = Widget::Countdown {
            target: target.into(),
            label: String::new(),
            done: done.into(),
            style: style(),
        };
        match show(&widget, KeyNow::default(), now) {
            WidgetContent::Countdown { main, detail, .. } => (main, detail),
            other => panic!("not a countdown: {other:?}"),
        }
    }

    const TARGET: &str = "2026-12-31T23:59";

    fn before_target(seconds: i64) -> Timestamp {
        let at = TARGET
            .parse::<DateTime>()
            .unwrap()
            .to_zoned(TimeZone::system())
            .unwrap()
            .timestamp();
        Timestamp::from_millisecond(at.as_millisecond() - seconds * 1000).unwrap()
    }

    fn readings(cpu: f32) -> Readings {
        Readings {
            cpu: Some(cpu),
            ..Readings::default()
        }
    }

    #[test]
    fn digital_clock_in_24_and_12_hours() {
        let WidgetContent::DigitalClock {
            time,
            seconds,
            suffix,
            label,
            line,
        } = digital(false, ClockLine::None, Some("UTC"))
        else {
            panic!("not a digital clock");
        };
        assert_eq!(time, "13:05");
        assert_eq!(seconds.as_deref(), Some("09"));
        assert_eq!(suffix, None);
        assert_eq!(label.as_deref(), Some("Madrid"));
        assert_eq!(line, None);
        assert!(matches!(
            digital(true, ClockLine::None, Some("UTC")),
            WidgetContent::DigitalClock { time, suffix: Some(s), .. } if time == "1:05" && s == "PM"
        ));
        assert!(matches!(
            digital(true, ClockLine::None, Some("Asia/Tokyo")),
            WidgetContent::DigitalClock { time, suffix: Some(s), .. } if time == "10:05" && s == "PM"
        ));
        assert!(matches!(
            digital(false, ClockLine::None, Some("America/New_York")),
            WidgetContent::DigitalClock { time, .. } if time == "08:05"
        ));
    }

    #[test]
    fn twelve_hour_clock_around_midnight_and_noon() {
        let at = |time: &str| {
            let widget = Widget::DigitalClock {
                seconds: false,
                hour12: true,
                second_line: ClockLine::None,
                zone: Some("UTC".into()),
                label: String::new(),
                style: style(),
            };
            let now = format!("2026-03-04T{time}Z").parse().unwrap();
            match show(&widget, KeyNow::default(), now) {
                WidgetContent::DigitalClock {
                    time,
                    suffix,
                    seconds,
                    label,
                    ..
                } => {
                    assert_eq!((seconds, label), (None, None));
                    format!("{time} {}", suffix.unwrap())
                }
                other => panic!("not a digital clock: {other:?}"),
            }
        };
        assert_eq!(at("00:07:00"), "12:07 AM");
        assert_eq!(at("11:59:00"), "11:59 AM");
        assert_eq!(at("12:00:00"), "12:00 PM");
        assert_eq!(at("23:30:00"), "11:30 PM");
    }

    #[test]
    fn second_line_in_spanish() {
        let line = |second_line| match digital(false, second_line, Some("UTC")) {
            WidgetContent::DigitalClock { line, .. } => line,
            other => panic!("not a digital clock: {other:?}"),
        };
        assert_eq!(line(ClockLine::Date).as_deref(), Some("4 de marzo de 2026"));
        assert_eq!(line(ClockLine::Weekday).as_deref(), Some("Miércoles"));
        assert_eq!(
            line(ClockLine::WeekdayDate).as_deref(),
            Some("Miércoles, 4 de marzo")
        );
        assert_eq!(line(ClockLine::None), None);
    }

    #[test]
    fn an_unknown_zone_uses_the_macs_own() {
        assert_eq!(
            digital(false, ClockLine::Date, Some("Nowhere/Atlantis")),
            digital(false, ClockLine::Date, None)
        );
        assert_eq!(
            digital(false, ClockLine::Date, Some("  ")),
            digital(false, ClockLine::Date, None)
        );
    }

    #[test]
    fn analog_clock_angles() {
        let analog = |seconds, date| {
            let widget = Widget::AnalogClock {
                seconds,
                date,
                zone: Some("UTC".into()),
                label: String::new(),
                style: style(),
            };
            show(&widget, KeyNow::default(), now())
        };
        let WidgetContent::AnalogClock {
            hour,
            minute,
            second,
            label,
            weekday,
            date,
        } = analog(true, true)
        else {
            panic!("not an analog clock");
        };
        let close = |a: f32, b: f32| (a - b).abs() < 1e-3;
        assert!(close(second.unwrap(), 54.0));
        assert!(close(minute, 5.15 * 6.0), "{minute}");
        assert!(close(hour, (1.0 + 5.15 / 60.0) * 30.0), "{hour}");
        assert_eq!(label, None);
        assert_eq!(weekday.as_deref(), Some("Miércoles"));
        assert_eq!(date.as_deref(), Some("4 de marzo"));
        let WidgetContent::AnalogClock {
            hour,
            minute,
            second,
            weekday,
            date,
            ..
        } = analog(false, false)
        else {
            panic!("not an analog clock");
        };
        assert_eq!((second, weekday, date), (None, None, None));
        assert!(close(minute, 30.0));
        assert!(close(hour, 32.5));
    }

    #[test]
    fn countdown_rounds_up_to_whole_seconds() {
        let at = before_target(0);
        let shifted = |millis| Timestamp::from_millisecond(at.as_millisecond() + millis).unwrap();
        assert_eq!(left_until(TARGET, shifted(-1500)), Some(2));
        assert_eq!(left_until(TARGET, shifted(-1000)), Some(1));
        assert_eq!(left_until(TARGET, at), Some(0));
        assert_eq!(left_until(TARGET, shifted(500)), Some(0));
        assert_eq!(left_until(TARGET, shifted(1000)), Some(-1));
        assert_eq!(left_until(&format!(" {TARGET} "), at), Some(0));
        assert_eq!(left_until("mañana", at), None);
        assert_eq!(left_until("", at), None);
    }

    #[test]
    fn countdown_shows_days_or_the_clock() {
        let clock = 3 * 3600 + 2 * 60 + 5;
        assert_eq!(
            countdown(TARGET, "", before_target(clock)),
            ("03:02:05".into(), None)
        );
        assert_eq!(
            countdown(TARGET, "", before_target(86_400 + 5)),
            ("1 día".into(), Some("00:00:05".into()))
        );
        assert_eq!(
            countdown(TARGET, "", before_target(3 * 86_400 + 3600)),
            ("3 días".into(), Some("01:00:00".into()))
        );
    }

    #[test]
    fn countdown_once_done_or_unreadable() {
        assert_eq!(
            countdown(TARGET, " ¡Feliz año! ", before_target(0)),
            ("¡Feliz año!".into(), None)
        );
        assert_eq!(
            countdown(TARGET, "  ", before_target(-60)),
            (done_text().into(), None)
        );
        assert_eq!(
            countdown("31/12/2026", "", before_target(10)),
            (no_date().into(), None)
        );
    }

    #[test]
    fn rates_in_decimal_units_with_a_comma_below_ten() {
        let shown = |bytes| {
            let rate = rate(bytes);
            format!("{} {}", rate.value, rate.unit)
        };
        assert_eq!(shown(None), "— ");
        assert_eq!(shown(Some(0.0)), "0 B/s");
        assert_eq!(shown(Some(5.4)), "5 B/s");
        assert_eq!(shown(Some(999.0)), "999 B/s");
        assert_eq!(shown(Some(1000.0)), "1,0 KB/s");
        assert_eq!(shown(Some(9_440.0)), "9,4 KB/s");
        assert_eq!(shown(Some(12_345.0)), "12 KB/s");
        assert_eq!(shown(Some(1.5e6)), "1,5 MB/s");
        assert_eq!(shown(Some(250e6)), "250 MB/s");
        assert_eq!(shown(Some(2.5e9)), "2,5 GB/s");
    }

    #[test]
    fn window_pads_missing_readings_at_the_start() {
        let history: Vec<Readings> = (0..10).map(|i| readings(i as f32 / 10.0)).collect();
        let window = window(history.clone(), 1);
        assert_eq!(window.len(), 30);
        assert!(window[..20].iter().all(|r| *r == Readings::default()));
        assert_eq!(window[20..], history[..]);
    }

    #[test]
    fn window_keeps_only_the_last_minutes() {
        let history: Vec<Readings> = (0..50).map(|i| readings(i as f32)).collect();
        let window = window(history.clone(), 1);
        assert_eq!(window.len(), 30);
        assert_eq!(window[..], history[20..]);
    }

    #[test]
    fn window_averages_down_to_the_graph_points() {
        let history: Vec<Readings> = (0..1800).map(|i| readings(i as f32)).collect();
        let window = window(history, 60);
        assert_eq!(window.len(), GRAPH_POINTS);
        assert_eq!(window[0].cpu, Some(7.0));
        assert_eq!(window[119].cpu, Some(1792.0));
        assert_eq!(window[0].down, None);

        let mut gaps = vec![readings(1.0), Readings::default(), readings(3.0)];
        gaps.extend(std::iter::repeat_n(readings(0.0), 1797));
        let first = super::window(gaps, 60)[0].cpu.unwrap();
        assert!((first - 4.0 / 14.0).abs() < 1e-6, "{first}");
    }

    #[test]
    fn window_clamps_the_minutes() {
        assert_eq!(window(Vec::new(), 0).len(), 30);
        let day = window(Vec::new(), u32::MAX);
        assert_eq!(day.len(), GRAPH_POINTS);
        assert!(day.iter().all(|r| *r == Readings::default()));
    }

    #[test]
    fn network_graph_scales_to_the_fastest_speed() {
        let speeds = |down, up| Readings {
            down,
            up,
            ..Readings::default()
        };
        let idle = network_graph(&[speeds(Some(500.0), Some(250.0)), speeds(None, None)]);
        assert_eq!(idle.down, [Some(0.5), None]);
        assert_eq!(idle.up, [Some(0.25), None]);
        assert_eq!(
            (idle.peak.value.as_str(), idle.peak.unit.as_str()),
            ("1,0", "KB/s")
        );

        let busy = network_graph(&[speeds(Some(4e6), Some(1e6)), speeds(Some(2e6), Some(8e6))]);
        assert_eq!(busy.down, [Some(0.5), Some(0.25)]);
        assert_eq!(busy.up, [Some(0.125), Some(1.0)]);
        assert_eq!(
            (busy.peak.value.as_str(), busy.peak.unit.as_str()),
            ("8,0", "MB/s")
        );

        let empty = network_graph(&[]);
        assert!(empty.down.is_empty() && empty.up.is_empty());
        assert_eq!(empty.peak.value, "1,0");
    }

    fn content(widget: Widget, timer: TimerNow) -> WidgetContent {
        let key = KeyNow {
            timer,
            ..KeyNow::default()
        };
        show(&widget, key, now())
    }

    #[test]
    fn world_clocks_tell_the_day_there() {
        let clock = |zone: &str, label: &str| WorldClock {
            zone: zone.into(),
            label: label.into(),
        };
        let widget = Widget::WorldClocks {
            clocks: vec![
                clock("Pacific/Kiritimati", ""),
                clock("America/New_York", "NY"),
                clock("Pacific/Pago_Pago", " "),
                clock("Europe/Madrid", "extra"),
            ],
            hour12: true,
            style: style(),
        };
        let WidgetContent::WorldClocks { clocks } = content(widget, TimerNow::Idle) else {
            panic!()
        };
        assert_eq!(clocks.len(), 3, "three at most");
        assert_eq!(
            (clocks[0].label.as_str(), clocks[0].time.as_str()),
            ("Kiritimati", "3:05")
        );
        assert!(clocks[0].night);
        assert_eq!(
            (clocks[1].label.as_str(), clocks[1].time.as_str()),
            ("NY", "8:05")
        );
        assert_eq!(clocks[1].suffix.as_deref(), Some("AM"));
        assert!(!clocks[1].night);
        assert_eq!(clocks[2].label, "Pago Pago", "a blank name shows the city");

        let madrid = TimeZone::get("Europe/Madrid").unwrap();
        let day =
            |zone: &str, at: &str| face(&clock(zone, ""), false, at.parse().unwrap(), &madrid).day;
        assert_eq!(
            day("Pacific/Kiritimati", "2026-03-04T13:05:00Z").as_deref(),
            Some("mañana")
        );
        assert_eq!(
            day("America/New_York", "2026-03-04T01:00:00Z").as_deref(),
            Some("ayer")
        );
        assert_eq!(day("America/New_York", "2026-03-04T13:05:00Z"), None);
    }

    #[test]
    fn the_calendar_lays_the_month_from_monday() {
        let today = "2026-03-04T12:00:00[Europe/Madrid]"
            .parse::<Zoned>()
            .unwrap();
        let WidgetContent::Calendar {
            weekday,
            day,
            month,
            weeks,
            today,
        } = calendar(&today)
        else {
            panic!()
        };
        assert_eq!(
            (weekday.as_str(), day.as_str(), month.as_str()),
            ("Miércoles", "4", "Marzo 2026")
        );
        assert_eq!(today, 4);
        assert_eq!(weeks.len(), 6);
        assert_eq!(weeks[0], [0, 0, 0, 0, 0, 0, 1], "1 March 2026 is a Sunday");
        assert_eq!(weeks[5], [30, 31, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn the_timer_shows_where_it_is() {
        let widget = || Widget::Timer {
            seconds: 25 * 60,
            label: "Pomodoro".into(),
            done: String::new(),
            style: style(),
        };
        let shows = |timer| match content(widget(), timer) {
            WidgetContent::Timer {
                main,
                progress,
                phase,
                ..
            } => (main, progress, phase),
            other => panic!("{other:?}"),
        };
        assert_eq!(
            shows(TimerNow::Idle),
            ("25:00".into(), 0.0, TimerPhase::Idle)
        );
        let (main, progress, phase) = shows(TimerNow::Running(Duration::from_millis(299_500)));
        assert_eq!(
            (main.as_str(), phase),
            ("05:00", TimerPhase::Running),
            "seconds round up"
        );
        assert!((progress - 0.8).abs() < 0.01);
        assert_eq!(
            shows(TimerNow::Paused(Duration::from_secs(4000))).0,
            "1:06:40"
        );
        assert_eq!(
            shows(TimerNow::Done),
            ("¡Tiempo!".into(), 1.0, TimerPhase::Done)
        );
    }

    #[test]
    fn the_battery_says_how_it_is() {
        let shows = |reading| match battery(reading) {
            WidgetContent::Battery {
                text, detail, low, ..
            } => (text, detail, low),
            other => panic!("{other:?}"),
        };
        let reading = |percent, power, minutes_left| {
            Some(Some(Battery {
                percent,
                power,
                minutes_left,
            }))
        };
        assert_eq!(
            shows(reading(62, Power::Charging, Some(83))),
            ("62 %".into(), "Cargando · llena en 1:23".into(), false)
        );
        assert_eq!(shows(reading(100, Power::Plugged, None)).1, "Cargada");
        assert_eq!(
            shows(reading(15, Power::Draining, Some(40))),
            ("15 %".into(), "Quedan 0:40".into(), true)
        );
        assert_eq!(shows(Some(None)).1, "Sin batería");
        assert_eq!(shows(None).1, "", "not read yet");
    }

    #[test]
    fn now_playing_moves_along_while_it_plays() {
        let read_at = Instant::now();
        let track = Track {
            title: "So What".into(),
            artist: "Miles Davis".into(),
            playing: true,
            position: Some(60.0),
            duration: Some(240.0),
            read_at,
        };
        let WidgetContent::NowPlaying {
            title,
            progress,
            time,
            ..
        } = now_playing(Some(Some(track)), read_at + Duration::from_secs(60))
        else {
            panic!()
        };
        assert_eq!(title.as_deref(), Some("So What"));
        assert_eq!(progress, Some(0.5));
        assert_eq!(time.as_deref(), Some("2:00 / 4:00"));
        let WidgetContent::NowPlaying { title, artist, .. } = now_playing(Some(None), read_at)
        else {
            panic!()
        };
        assert_eq!((title, artist.as_str()), (None, "Nada sonando"));
    }

    #[test]
    fn the_moon_follows_its_month() {
        let at = |when: &str| match moon(when.parse().unwrap()) {
            WidgetContent::Moon {
                phase,
                name,
                detail,
            } => (phase, name, detail),
            other => panic!("{other:?}"),
        };
        let (phase, name, _) = at("2000-01-06T18:14:00Z");
        assert!(phase < 0.001, "{phase}");
        assert_eq!(name, "Luna nueva");
        let (_, name, detail) = at("2026-03-03T11:38:00Z");
        assert_eq!(name, "Luna llena", "the full moon of 3 March 2026");
        assert!(detail.starts_with("100 %"), "{detail}");
        let (_, name, detail) = at("2026-02-24T12:00:00Z");
        assert_eq!(name, "Cuarto creciente");
        assert!(detail.contains("llena en 7 días"), "{detail}");
    }

    #[test]
    fn the_word_clock_rounds_to_five_minutes() {
        let words = |time: &str| match word_clock(
            &format!("2026-03-04T{time}[Europe/Madrid]").parse().unwrap(),
        ) {
            WidgetContent::WordClock { lead, hour, rest } => format!("{lead} {hour} {rest}"),
            other => panic!("{other:?}"),
        };
        assert_eq!(words("15:14:00"), "Son las tres y cuarto");
        assert_eq!(words("13:02:00"), "Es la una en punto");
        assert_eq!(words("02:33:00"), "Son las tres menos veinticinco");
        assert_eq!(words("12:58:00"), "Es la una en punto");
        assert_eq!(words("23:47:00"), "Son las doce menos cuarto");
        assert_eq!(words("00:30:00"), "Son las doce y media");
    }

    #[test]
    fn the_word_clock_in_english() {
        i18n::set(Language::En);
        let words = |time: &str| match word_clock(
            &format!("2026-03-04T{time}[Europe/Madrid]").parse().unwrap(),
        ) {
            WidgetContent::WordClock { lead, hour, rest } => (lead, hour, rest),
            other => panic!("{other:?}"),
        };
        let line = |time: &str| {
            let (lead, hour, rest) = words(time);
            [lead, hour, rest]
                .into_iter()
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
        };
        assert_eq!(
            words("15:01:00"),
            ("It's".into(), "three".into(), "o'clock".into())
        );
        assert_eq!(
            words("15:14:00"),
            ("It's quarter past".into(), "three".into(), String::new())
        );
        assert_eq!(line("15:31:00"), "It's half past three");
        assert_eq!(line("15:40:00"), "It's twenty to four");
        assert_eq!(line("23:47:00"), "It's quarter to twelve");
        assert_eq!(line("00:05:00"), "It's five past twelve");
        assert_eq!(line("12:58:00"), "It's one o'clock");
        assert_eq!(line("02:33:00"), "It's twenty-five to three");
    }

    #[test]
    fn dates_in_english() {
        i18n::set(Language::En);
        let line = |second_line| match digital(false, second_line, Some("UTC")) {
            WidgetContent::DigitalClock { line, .. } => line,
            other => panic!("not a digital clock: {other:?}"),
        };
        assert_eq!(line(ClockLine::Date).as_deref(), Some("March 4, 2026"));
        assert_eq!(line(ClockLine::Weekday).as_deref(), Some("Wednesday"));
        assert_eq!(
            line(ClockLine::WeekdayDate).as_deref(),
            Some("Wednesday, March 4")
        );
        let today = "2026-03-04T12:00:00[Europe/Madrid]"
            .parse::<Zoned>()
            .unwrap();
        let WidgetContent::Calendar { weekday, month, .. } = calendar(&today) else {
            panic!()
        };
        assert_eq!(
            (weekday.as_str(), month.as_str()),
            ("Wednesday", "March 2026")
        );
        assert_eq!(countdown(TARGET, "", before_target(86_400 + 5)).0, "1 day");
        assert_eq!(countdown(TARGET, "", before_target(3 * 86_400)).0, "3 days");
        assert_eq!(countdown(TARGET, "  ", before_target(-60)).0, "Done!");
        let madrid = TimeZone::get("Europe/Madrid").unwrap();
        let clock = WorldClock {
            zone: "Pacific/Kiritimati".into(),
            label: String::new(),
        };
        let at = "2026-03-04T13:05:00Z".parse().unwrap();
        assert_eq!(
            face(&clock, false, at, &madrid).day.as_deref(),
            Some("tomorrow")
        );
    }

    #[test]
    fn english_uses_a_decimal_point_and_its_own_labels() {
        i18n::set(Language::En);
        let rate = rate(Some(1.5e6));
        assert_eq!(format!("{} {}", rate.value, rate.unit), "1.5 MB/s");
        let WidgetContent::Latency { value, .. } = latency(
            "",
            Some(Some(Pings {
                target: "192.168.1.1".into(),
                history: vec![Some(4.25)],
            })),
        ) else {
            panic!()
        };
        assert_eq!(value, "4.2 ms");
        let WidgetContent::Latency { value, .. } = latency(
            "nas",
            Some(Some(Pings {
                target: "nas".into(),
                history: vec![None],
            })),
        ) else {
            panic!()
        };
        assert_eq!(value, "No reply");

        let battery = |percent, power, minutes_left| match battery(Some(Some(Battery {
            percent,
            power,
            minutes_left,
        }))) {
            WidgetContent::Battery { detail, .. } => detail,
            other => panic!("{other:?}"),
        };
        assert_eq!(
            battery(62, Power::Charging, Some(83)),
            "Charging · full in 1:23"
        );
        assert_eq!(battery(15, Power::Draining, Some(40)), "0:40 left");

        let now: Zoned = "2026-03-04T12:00:00[Europe/Madrid]".parse().unwrap();
        let WidgetContent::Meters { meters, .. } = progress(
            &[Period::Day, Period::Week, Period::Month, Period::Year],
            &now,
        ) else {
            panic!()
        };
        let labels: Vec<&str> = meters.iter().map(|m| m.label.as_str()).collect();
        assert_eq!(labels, ["Day", "Week", "Month", "Year"]);
        assert_eq!(meter(Metric::Disk, &Readings::default(), &[]).label, "Disk");

        let WidgetContent::Moon { name, detail, .. } =
            moon("2026-02-24T12:00:00Z".parse().unwrap())
        else {
            panic!()
        };
        assert_eq!(name, "First quarter");
        assert!(detail.contains("full in 7 days"), "{detail}");
        i18n::set(Language::Es);
    }

    #[test]
    fn progress_of_the_day_week_month_and_year() {
        let now: Zoned = "2026-03-04T12:00:00[Europe/Madrid]".parse().unwrap();
        let WidgetContent::Meters { meters, .. } = progress(
            &[
                Period::Day,
                Period::Week,
                Period::Month,
                Period::Year,
                Period::Day,
            ],
            &now,
        ) else {
            panic!()
        };
        let shown: Vec<(&str, &str)> = meters
            .iter()
            .map(|m| (m.label.as_str(), m.text.as_str()))
            .collect();
        assert_eq!(
            shown,
            vec![
                ("Día", "50 %"),
                ("Semana", "35 %"),
                ("Mes", "11 %"),
                ("Año", "17 %")
            ],
            "four at most"
        );
    }

    #[test]
    fn phrases_turn_with_time_and_presses() {
        let list = vec!["Uno".to_owned(), "  ".into(), "Dos".into(), "Tres".into()];
        let at = |seconds, presses, when: i64| match phrase(
            &list,
            seconds,
            presses,
            Timestamp::from_second(when).unwrap(),
        ) {
            WidgetContent::Phrase { text } => text,
            other => panic!("{other:?}"),
        };
        assert_eq!(at(0, 0, 1000), "Uno", "blank ones are skipped");
        assert_eq!(at(0, 4, 1000), "Dos");
        assert_eq!(at(10, 0, 0), "Uno");
        assert_eq!(at(10, 0, 10), "Dos");
        assert_eq!(at(10, 1, 10), "Tres");
        assert_eq!(
            phrase(&[], 5, 0, now()),
            WidgetContent::Phrase {
                text: "Escribe tus frases".into()
            }
        );
    }

    #[test]
    fn latency_scales_the_answers() {
        let pings = Pings {
            target: "192.168.1.1".into(),
            history: vec![Some(5.0), None, Some(50.0), Some(4.25)],
        };
        let WidgetContent::Latency {
            value,
            target,
            history,
            peak,
            ok,
        } = latency("", Some(Some(pings)))
        else {
            panic!()
        };
        assert_eq!(
            (value.as_str(), target.as_str(), peak.as_str()),
            ("4,2 ms", "Router · 192.168.1.1", "60 ms")
        );
        assert!(ok);
        assert_eq!(history.len(), crate::monitor::PING_HISTORY);
        assert_eq!(
            history
                .last()
                .copied()
                .flatten()
                .map(|v| (v * 100.0).round()),
            Some(7.0)
        );
        let WidgetContent::Latency { target, .. } = latency("", Some(None)) else {
            panic!()
        };
        assert_eq!(target, "Sin router");
        let WidgetContent::Latency { value, ok, .. } = latency(
            "nas",
            Some(Some(Pings {
                target: "nas".into(),
                history: vec![None],
            })),
        ) else {
            panic!()
        };
        assert_eq!((value.as_str(), ok), ("Sin respuesta", false));
    }

    #[test]
    fn sensors_processes_and_volume_as_bars() {
        let WidgetContent::Meters { meters, .. } = temperatures(Some(Some(Temperatures {
            cpu: Some(60.0),
            disk: None,
            battery: Some(30.4),
        }))) else {
            panic!()
        };
        let shown: Vec<(&str, &str)> = meters
            .iter()
            .map(|m| (m.label.as_str(), m.text.as_str()))
            .collect();
        assert_eq!(
            shown,
            vec![("CPU", "60 °C"), ("Batería", "30 °C")],
            "missing sensors are left out"
        );
        assert_eq!(meters[0].level, Some(0.5));

        let processes = Processes {
            by_cpu: vec![
                crate::monitor::Process {
                    name: "Google Chrome Helper (Renderer)".into(),
                    share: 0.25,
                },
                crate::monitor::Process {
                    name: "Finder".into(),
                    share: 0.031,
                },
            ],
            by_memory: Vec::new(),
        };
        let WidgetContent::Meters { meters, .. } = top_processes(Some(Some(processes)), Usage::Cpu)
        else {
            panic!()
        };
        let shown: Vec<(&str, &str)> = meters
            .iter()
            .map(|m| (m.label.as_str(), m.text.as_str()))
            .collect();
        assert_eq!(shown, vec![("Google Chrome…", "25 %"), ("Finder", "3,1 %")]);

        let WidgetContent::Meters { meters, .. } = volume(Some(Some(Volume {
            output: Some(40),
            input: None,
            muted: true,
        }))) else {
            panic!()
        };
        let shown: Vec<(&str, &str)> = meters
            .iter()
            .map(|m| (m.label.as_str(), m.text.as_str()))
            .collect();
        assert_eq!(shown, vec![("Altavoz", "Silencio"), ("Micro", "—")]);
    }

    #[test]
    fn stopwatches_and_counters_show_their_key() {
        let watch = |elapsed: u64, running| match stopwatch(
            None,
            StopwatchNow {
                elapsed: Duration::from_secs(elapsed),
                running,
            },
        ) {
            WidgetContent::Countdown { main, detail, .. } => (main, detail),
            other => panic!("{other:?}"),
        };
        assert_eq!(
            watch(0, false),
            ("00:00".into(), Some("Pulsa para empezar".into()))
        );
        assert_eq!(watch(75, true), ("01:15".into(), None));
        assert_eq!(
            watch(3725, false),
            ("1:02:05".into(), Some("En pausa".into()))
        );
        let counter = Widget::Counter {
            label: "Cafés".into(),
            step: 1,
            style: style(),
        };
        let key = KeyNow {
            count: -3,
            ..KeyNow::default()
        };
        assert_eq!(
            show(&counter, key, now()),
            WidgetContent::Countdown {
                label: Some("Cafés".into()),
                main: "-3".into(),
                detail: None
            }
        );
    }
}

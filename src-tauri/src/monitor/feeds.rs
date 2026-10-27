//! Readings that take a process to get (the battery, the song playing), taken on a thread of
//! their own and only while some key shows them, so drawing never waits for them.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use super::platform;

/// A feed nobody asked for in this long stops being read.
const WANTED_FOR: Duration = Duration::from_secs(10);
const TRACK_EVERY: Duration = Duration::from_secs(1);
const BATTERY_EVERY: Duration = Duration::from_secs(20);
const VOLUME_EVERY: Duration = Duration::from_secs(1);
const PING_EVERY: Duration = Duration::from_secs(2);
const PROCESSES_EVERY: Duration = Duration::from_secs(2);
const TEMPERATURES_EVERY: Duration = Duration::from_secs(5);
/// Two minutes of pings.
pub const PING_HISTORY: usize = 60;
const TOP: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Power {
    Charging,
    /// Plugged in and full, or holding its charge to spare the battery.
    Plugged,
    Draining,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Battery {
    pub percent: u8,
    pub power: Power,
    /// Until full while charging, or until empty while draining, when macOS knows.
    pub minutes_left: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    pub title: String,
    pub artist: String,
    pub playing: bool,
    /// Seconds into the song, when it was read.
    pub position: Option<f32>,
    pub duration: Option<f32>,
    /// When it was read, to move the position along between readings.
    pub read_at: Instant,
}

impl Track {
    /// Where the song is now, guessing from the last reading while it plays.
    pub fn position_now(&self, now: Instant) -> Option<f32> {
        let position = self.position?;
        let moved = if self.playing {
            now.saturating_duration_since(self.read_at).as_secs_f32()
        } else {
            0.0
        };
        Some(match self.duration {
            Some(duration) => (position + moved).min(duration),
            None => position + moved,
        })
    }
}

/// None until first read; then what was found, which may be nothing (a Mac without a battery,
/// no song playing).
struct Feed<T> {
    value: Option<Option<T>>,
    read_at: Option<Instant>,
    asked_at: Option<Instant>,
}

impl<T> Default for Feed<T> {
    fn default() -> Self {
        Self {
            value: None,
            read_at: None,
            asked_at: None,
        }
    }
}

impl<T: Clone> Feed<T> {
    fn ask(&mut self, now: Instant) -> Option<Option<T>> {
        self.asked_at = Some(now);
        self.value.clone()
    }

    fn due(&self, every: Duration, now: Instant) -> bool {
        let wanted = self
            .asked_at
            .is_some_and(|at| now.saturating_duration_since(at) < WANTED_FOR);
        wanted
            && self
                .read_at
                .is_none_or(|at| now.saturating_duration_since(at) >= every)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Volume {
    /// None when the output has no volume of its own, such as a display over HDMI.
    pub output: Option<u8>,
    pub input: Option<u8>,
    pub muted: bool,
}

/// Milliseconds of each answer, oldest first; None where it did not answer.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Pings {
    /// The address asked, once the router is found.
    pub target: String,
    pub history: Vec<Option<f32>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Process {
    pub name: String,
    /// Share of the whole processor or of the memory, from 0 to 1.
    pub share: f32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Processes {
    pub by_cpu: Vec<Process>,
    pub by_memory: Vec<Process>,
}

/// Degrees Celsius.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Temperatures {
    pub cpu: Option<f32>,
    pub disk: Option<f32>,
    pub battery: Option<f32>,
}

/// What was last shown of a host, and its answers so far.
type PingFeed = (Feed<Pings>, VecDeque<Option<f32>>);

#[derive(Default)]
pub struct Feeds {
    battery: Mutex<Feed<Battery>>,
    track: Mutex<Feed<Track>>,
    volume: Mutex<Feed<Volume>>,
    processes: Mutex<Feed<Processes>>,
    temperatures: Mutex<Feed<Temperatures>>,
    /// By host as the user wrote it.
    pings: Mutex<HashMap<String, PingFeed>>,
    started: AtomicBool,
}

impl Feeds {
    pub fn battery(self: &Arc<Self>) -> Option<Option<Battery>> {
        self.start();
        lock(&self.battery).ask(Instant::now())
    }

    pub fn track(self: &Arc<Self>) -> Option<Option<Track>> {
        self.start();
        lock(&self.track).ask(Instant::now())
    }

    pub fn volume(self: &Arc<Self>) -> Option<Option<Volume>> {
        self.start();
        lock(&self.volume).ask(Instant::now())
    }

    pub fn processes(self: &Arc<Self>) -> Option<Option<Processes>> {
        self.start();
        lock(&self.processes).ask(Instant::now())
    }

    pub fn temperatures(self: &Arc<Self>) -> Option<Option<Temperatures>> {
        self.start();
        lock(&self.temperatures).ask(Instant::now())
    }

    pub fn pings(self: &Arc<Self>, host: &str) -> Option<Option<Pings>> {
        self.start();
        let mut pings = lock(&self.pings);
        pings
            .entry(host.to_owned())
            .or_default()
            .0
            .ask(Instant::now())
    }

    fn start(self: &Arc<Self>) {
        if self.started.swap(true, Ordering::Relaxed) {
            return;
        }
        let feeds = Arc::clone(self);
        let spawned = thread::Builder::new().name("feeds".into()).spawn(move || {
            let mut system = SystemFeeds::default();
            loop {
                let now = Instant::now();
                if lock(&feeds.track).due(TRACK_EVERY, now) {
                    let track = platform::now_playing();
                    let mut feed = lock(&feeds.track);
                    feed.value = Some(track);
                    feed.read_at = Some(now);
                }
                if lock(&feeds.battery).due(BATTERY_EVERY, now) {
                    let battery = platform::battery();
                    let mut feed = lock(&feeds.battery);
                    feed.value = Some(battery);
                    feed.read_at = Some(now);
                }
                if lock(&feeds.volume).due(VOLUME_EVERY, now) {
                    let volume = platform::volume();
                    let mut feed = lock(&feeds.volume);
                    feed.value = Some(volume);
                    feed.read_at = Some(now);
                }
                if lock(&feeds.processes).due(PROCESSES_EVERY, now) {
                    let processes = system.processes();
                    let mut feed = lock(&feeds.processes);
                    feed.value = Some(Some(processes));
                    feed.read_at = Some(now);
                }
                if lock(&feeds.temperatures).due(TEMPERATURES_EVERY, now) {
                    let temperatures = system.temperatures();
                    let mut feed = lock(&feeds.temperatures);
                    feed.value = Some(Some(temperatures));
                    feed.read_at = Some(now);
                }
                let hosts: Vec<String> = lock(&feeds.pings)
                    .iter()
                    .filter(|(_, (feed, _))| feed.due(PING_EVERY, now))
                    .map(|(host, _)| host.clone())
                    .collect();
                for host in hosts {
                    let target = if host.trim().is_empty() {
                        platform::router()
                    } else {
                        Some(host.trim().to_owned()).filter(|h| safe_host(h))
                    };
                    let answer = target.as_deref().and_then(platform::ping);
                    let mut pings = lock(&feeds.pings);
                    let (feed, history) = pings.entry(host).or_default();
                    history.push_back(answer);
                    while history.len() > PING_HISTORY {
                        history.pop_front();
                    }
                    feed.value = Some(target.map(|target| Pings {
                        target,
                        history: history.iter().copied().collect(),
                    }));
                    feed.read_at = Some(now);
                }
                lock(&feeds.pings).retain(|_, (feed, _)| {
                    feed.asked_at
                        .is_some_and(|at| now.saturating_duration_since(at) < WANTED_FOR)
                });
                thread::sleep(Duration::from_millis(250));
            }
        });
        if let Err(err) = spawned {
            log::warn!("could not read the battery or the music in the background: {err}");
        }
    }
}

/// The processes and sensors, read with the same handles every time: processor use needs the
/// previous reading.
#[derive(Default)]
struct SystemFeeds {
    system: Option<sysinfo::System>,
    components: Option<sysinfo::Components>,
}

impl SystemFeeds {
    fn processes(&mut self) -> Processes {
        use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, RefreshKind, System};
        let system = self.system.get_or_insert_with(|| {
            System::new_with_specifics(RefreshKind::nothing().with_memory(Default::default()))
        });
        system.refresh_memory();
        system.refresh_cpu_list(sysinfo::CpuRefreshKind::nothing());
        system.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::nothing().with_cpu().with_memory(),
        );
        let cpus = system.cpus().len().max(1) as f32;
        let memory = system.total_memory().max(1) as f32;
        let mut by_name: HashMap<String, (f32, u64)> = HashMap::new();
        for process in system.processes().values() {
            let entry = by_name
                .entry(process.name().to_string_lossy().into_owned())
                .or_default();
            entry.0 += process.cpu_usage();
            entry.1 += process.memory();
        }
        let top = |share: &dyn Fn(&(f32, u64)) -> f32| {
            let mut list: Vec<Process> = by_name
                .iter()
                .map(|(name, usage)| Process {
                    name: name.clone(),
                    share: share(usage).clamp(0.0, 1.0),
                })
                .collect();
            list.sort_by(|a, b| b.share.total_cmp(&a.share).then(a.name.cmp(&b.name)));
            list.truncate(TOP);
            list
        };
        Processes {
            by_cpu: top(&|(cpu, _)| cpu / 100.0 / cpus),
            by_memory: top(&|(_, bytes)| *bytes as f32 / memory),
        }
    }

    fn temperatures(&mut self) -> Temperatures {
        let components = self
            .components
            .get_or_insert_with(sysinfo::Components::new_with_refreshed_list);
        components.refresh(true);
        let readings: Vec<(String, f32)> = components
            .list()
            .iter()
            .filter_map(|c| Some((c.label().to_lowercase(), c.temperature()?)))
            .filter(|(_, t)| t.is_finite() && *t > 0.0)
            .collect();
        temperatures_from(&readings)
    }
}

/// The hottest processor sensor, the disk's and the battery's, from the sensors' names: Apple
/// silicon calls its dies `tdie`, Intel Macs say `cpu`; on Linux each driver names its own
/// (`coretemp` for Intel, `k10temp` for AMD, `nvme` for disks).
pub fn temperatures_from(readings: &[(String, f32)]) -> Temperatures {
    const CPU: &[&str] = &[
        "tdie", "cpu", "peci", "coretemp", "k10temp", "zenpower", "tctl",
    ];
    const DISK: &[&str] = &["nand", "ssd", "disk", "nvme", "drivetemp"];
    let hottest = |pick: &dyn Fn(&str) -> bool| {
        readings
            .iter()
            .filter(|(label, _)| pick(label))
            .map(|(_, t)| *t)
            .reduce(f32::max)
    };
    Temperatures {
        cpu: hottest(&|l| CPU.iter().any(|name| l.contains(name)))
            .or_else(|| hottest(&|l| l.contains("tdev") || l.contains("pmu"))),
        disk: hottest(&|l| DISK.iter().any(|name| l.contains(name))),
        battery: hottest(&|l| l.contains("battery")),
    }
}

/// A host for `ping`: letters, digits, dots, dashes and colons, never an option.
pub fn safe_host(host: &str) -> bool {
    !host.is_empty()
        && !host.starts_with('-')
        && host.len() <= 253
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '_'))
}

/// The time of the answer in `ping`'s output, as in `time=12.345 ms`.
#[cfg(unix)]
pub fn parse_ping(output: &str) -> Option<f32> {
    let at = output.find("time=")? + "time=".len();
    output[at..].split_whitespace().next()?.parse::<f32>().ok()
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn reads_the_ping() {
        let ping = "PING 1.1.1.1 (1.1.1.1): 56 data bytes\n64 bytes from 1.1.1.1: icmp_seq=0 ttl=57 time=12.345 ms\n";
        assert_eq!(parse_ping(ping), Some(12.345));
        assert_eq!(parse_ping("Request timeout for icmp_seq 0"), None);
    }

    #[test]
    fn only_plain_hosts_are_pinged() {
        for host in ["router.lan", "1.1.1.1", "fe80::1", "my-nas"] {
            assert!(safe_host(host), "{host}");
        }
        for host in ["", "-f", "a b", "x;rm", "$(x)"] {
            assert!(!safe_host(host), "{host}");
        }
    }

    #[test]
    fn picks_the_temperatures_by_sensor_name() {
        let readings = [
            ("pmu tdie1".to_owned(), 41.0),
            ("pmu tdie4".to_owned(), 45.5),
            ("pmu tdev1".to_owned(), 60.0),
            ("gas gauge battery".to_owned(), 30.0),
            ("nand ch0 temp".to_owned(), 33.0),
        ];
        assert_eq!(
            temperatures_from(&readings),
            Temperatures {
                cpu: Some(45.5),
                disk: Some(33.0),
                battery: Some(30.0)
            }
        );
        let linux = [
            ("coretemp package id 0".to_owned(), 52.0),
            ("coretemp core 0".to_owned(), 49.0),
            ("nvme composite".to_owned(), 37.0),
            ("acpitz temp1".to_owned(), 27.8),
        ];
        assert_eq!(
            temperatures_from(&linux),
            Temperatures {
                cpu: Some(52.0),
                disk: Some(37.0),
                battery: None
            }
        );
        let only_devices = [("pmu tdev3".to_owned(), 38.0)];
        assert_eq!(temperatures_from(&only_devices).cpu, Some(38.0));
        assert_eq!(temperatures_from(&[]), Temperatures::default());
    }

    #[test]
    fn feeds_are_read_only_while_asked_for() {
        let mut feed = Feed::<u8>::default();
        let now = Instant::now();
        assert!(!feed.due(TRACK_EVERY, now), "nobody asked");
        assert_eq!(feed.ask(now), None, "not read yet");
        assert!(feed.due(TRACK_EVERY, now));
        feed.value = Some(Some(3));
        feed.read_at = Some(now);
        assert!(!feed.due(TRACK_EVERY, now + Duration::from_millis(500)));
        assert!(feed.due(TRACK_EVERY, now + TRACK_EVERY));
        assert!(
            !feed.due(TRACK_EVERY, now + WANTED_FOR + TRACK_EVERY),
            "nobody looks anymore"
        );
    }
}

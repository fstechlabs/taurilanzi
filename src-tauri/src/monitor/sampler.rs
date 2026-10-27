use std::collections::VecDeque;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use sysinfo::{Disk, DiskRefreshKind, Disks, Networks, System};

use super::feeds::{Battery, Feeds, Pings, Processes, Temperatures, Track, Volume};
use super::platform;

/// Newer readings are shared, so the dock and the editor asking at once sample only once.
const FRESH: Duration = Duration::from_millis(1500);
/// Usage and speeds averaged over a longer gap would not say what is happening now.
const STALE: Duration = Duration::from_secs(10);
/// How often readings are taken in the background for the graphs.
pub const SAMPLE_EVERY: Duration = Duration::from_secs(2);
/// Readings kept for the graphs: the longest window they offer, a day.
pub const HISTORY: usize = 30 * 60 * 24;
/// Interfaces that repeat traffic already counted on a real one, or never leave the Mac.
const VIRTUAL_INTERFACES: &[&str] = &[
    "lo", "utun", "awdl", "llw", "bridge", "gif", "stf", "anpi", "ipsec", "docker", "veth", "vmnet",
];

/// What the system monitor and network widgets show.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Readings {
    /// Fractions from 0 to 1.
    pub cpu: Option<f32>,
    pub ram: Option<f32>,
    pub gpu: Option<f32>,
    /// How full the startup volume is.
    pub disk: Option<f32>,
    /// Bytes per second over every network interface.
    pub down: Option<f64>,
    pub up: Option<f64>,
}

/// Samples the system on demand, while something on screen needs it, and in the background
/// while some graph needs a history.
#[derive(Default)]
pub struct Monitor {
    sampler: Arc<Mutex<Option<Sampler>>>,
    background: Arc<AtomicBool>,
    started: AtomicBool,
    feeds: Arc<Feeds>,
}

impl Monitor {
    /// Keeps taking readings every couple of seconds, or stops.
    pub fn keep_sampling(&self, on: bool) {
        self.background.store(on, Ordering::Relaxed);
        if !on {
            return;
        }
        if !self.started.swap(true, Ordering::Relaxed) {
            let monitor = Self {
                sampler: Arc::clone(&self.sampler),
                background: Arc::clone(&self.background),
                started: AtomicBool::new(true),
                feeds: Arc::clone(&self.feeds),
            };
            let spawned = thread::Builder::new()
                .name("monitor".into())
                .spawn(move || {
                    loop {
                        if monitor.background.load(Ordering::Relaxed) {
                            monitor.readings();
                        }
                        thread::sleep(SAMPLE_EVERY);
                    }
                });
            if let Err(err) = spawned {
                log::warn!("could not sample the system in the background: {err}");
            }
        }
    }

    /// None until first read, then the battery if the Mac has one.
    pub fn battery(&self) -> Option<Option<Battery>> {
        self.feeds.battery()
    }

    /// None until first read, then the song in Music or Spotify, if any.
    pub fn track(&self) -> Option<Option<Track>> {
        self.feeds.track()
    }

    pub fn volume(&self) -> Option<Option<Volume>> {
        self.feeds.volume()
    }

    pub fn processes(&self) -> Option<Option<Processes>> {
        self.feeds.processes()
    }

    pub fn temperatures(&self) -> Option<Option<Temperatures>> {
        self.feeds.temperatures()
    }

    /// The answers of `host`, or of the router when it is empty. Some(None) when there is
    /// nothing to ask: no router, or a host that cannot be pinged.
    pub fn pings(&self, host: &str) -> Option<Option<Pings>> {
        self.feeds.pings(host)
    }

    pub fn readings(&self) -> Readings {
        self.with_sampler(|sampler| sampler.last)
    }

    /// The latest readings, oldest first, the last one being now.
    pub fn history(&self) -> Vec<Readings> {
        self.with_sampler(|sampler| sampler.history.iter().copied().collect())
    }

    fn with_sampler<T>(&self, read: impl FnOnce(&Sampler) -> T) -> T {
        let mut sampler = self.sampler.lock().unwrap_or_else(PoisonError::into_inner);
        let now = Instant::now();
        let sampler = match sampler.as_mut() {
            Some(sampler) => {
                if now.duration_since(sampler.taken) >= FRESH {
                    sampler.sample(now);
                }
                sampler
            }
            None => sampler.insert(Sampler::new(now)),
        };
        read(sampler)
    }
}

struct Sampler {
    system: System,
    disks: Disks,
    networks: Networks,
    taken: Instant,
    last: Readings,
    history: VecDeque<Readings>,
}

impl Sampler {
    /// CPU usage and speeds need two samples, so the first reading has none.
    fn new(now: Instant) -> Self {
        let mut system = System::new();
        system.refresh_cpu_usage();
        let mut sampler = Self {
            system,
            disks: Disks::new_with_refreshed_list(),
            networks: Networks::new_with_refreshed_list(),
            taken: now,
            last: Readings::default(),
            history: VecDeque::new(),
        };
        sampler.last = Readings {
            gpu: platform::gpu_utilization(),
            ..sampler.steady()
        };
        sampler.history.push_back(sampler.last);
        sampler
    }

    fn sample(&mut self, now: Instant) {
        let elapsed = now.duration_since(self.taken);
        self.taken = now;
        self.system.refresh_cpu_usage();
        // Only the startup volume: asking an unreachable network volume could hang.
        if let Some(root) = self.root_disk() {
            root.refresh_specifics(DiskRefreshKind::nothing().with_storage());
        }
        self.networks.refresh(true);
        let recent = elapsed < STALE;
        // A graph across a long gap would join readings far apart in time.
        if !recent {
            self.history.clear();
        }
        let seconds = elapsed.as_secs_f64().max(0.001);
        let (down, up) = self
            .networks
            .iter()
            .filter(|(name, _)| !VIRTUAL_INTERFACES.iter().any(|v| name.starts_with(v)))
            .fold((0, 0), |(down, up), (_, data)| {
                (down + data.received(), up + data.transmitted())
            });
        self.last = Readings {
            cpu: recent.then(|| (self.system.global_cpu_usage() / 100.0).clamp(0.0, 1.0)),
            gpu: platform::gpu_utilization(),
            down: recent.then_some(down as f64 / seconds),
            up: recent.then_some(up as f64 / seconds),
            ..self.steady()
        };
        if self.history.len() == HISTORY {
            self.history.pop_front();
        }
        self.history.push_back(self.last);
    }

    /// Readings that need no previous sample.
    fn steady(&mut self) -> Readings {
        self.system.refresh_memory();
        let total = self.system.total_memory();
        let ram = (total > 0).then(|| self.system.used_memory() as f32 / total as f32);
        let disk = self
            .root_disk()
            .filter(|disk| disk.total_space() > 0)
            .map(|disk| 1.0 - disk.available_space() as f32 / disk.total_space() as f32);
        Readings {
            ram,
            disk,
            ..Readings::default()
        }
    }

    fn root_disk(&mut self) -> Option<&mut Disk> {
        let disks = self.disks.list_mut();
        let at = disks
            .iter()
            .position(|disk| disk.mount_point() == Path::new("/"))
            .or((!disks.is_empty()).then_some(0))?;
        disks.get_mut(at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_reading_has_no_usage_or_speeds() {
        let sampler = Sampler::new(Instant::now());
        assert_eq!(sampler.history.len(), 1);
        assert_eq!(sampler.history[0], sampler.last);
        assert_eq!(
            (sampler.last.cpu, sampler.last.down, sampler.last.up),
            (None, None, None)
        );
    }

    #[test]
    fn a_recent_sample_has_levels_within_range() {
        let start = Instant::now();
        let mut sampler = Sampler::new(start);
        sampler.sample(start + SAMPLE_EVERY);
        let last = sampler.last;
        for level in [last.cpu, last.ram, last.disk, last.gpu]
            .into_iter()
            .flatten()
        {
            assert!((0.0..=1.0).contains(&level), "{last:?}");
        }
        assert!(last.cpu.is_some());
        assert!(last.down.is_some_and(|d| d >= 0.0) && last.up.is_some_and(|u| u >= 0.0));
        assert_eq!(sampler.history.len(), 2);
        assert_eq!(sampler.history.back(), Some(&last));
    }

    #[test]
    fn history_never_grows_past_a_day() {
        let start = Instant::now();
        let mut sampler = Sampler::new(start);
        let old = Readings {
            cpu: Some(0.5),
            ..Readings::default()
        };
        sampler.history = std::iter::repeat_n(old, HISTORY).collect();
        sampler.sample(start + SAMPLE_EVERY);
        assert_eq!(sampler.history.len(), HISTORY);
        assert_eq!(sampler.history.back(), Some(&sampler.last));
        sampler.sample(start + SAMPLE_EVERY * 2);
        assert_eq!(sampler.history.len(), HISTORY);
    }

    #[test]
    fn a_long_gap_starts_the_history_again() {
        let start = Instant::now();
        let mut sampler = Sampler::new(start);
        sampler.sample(start + SAMPLE_EVERY);
        sampler.sample(start + SAMPLE_EVERY + STALE);
        assert_eq!(sampler.history.len(), 1);
        let last = sampler.last;
        assert_eq!((last.cpu, last.down, last.up), (None, None, None));
    }

    #[test]
    fn fresh_readings_are_shared() {
        let monitor = Monitor::default();
        let first = monitor.readings();
        assert_eq!(monitor.readings(), first);
        assert_eq!(monitor.history(), [first]);
    }
}

use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

use log::warn;
use rodio::cpal::traits::HostTrait;
use rodio::{Decoder, DeviceSinkBuilder, DeviceTrait, MixerDeviceSink, Player, Source};

use crate::config::SoundMode;

pub struct Play {
    /// Sounds are told apart by asset id, so a key's toggle finds what it started.
    pub id: String,
    pub path: PathBuf,
    /// 0.0–1.0.
    pub volume: f32,
    pub mode: SoundMode,
    pub device: Option<String>,
}

enum Request {
    Play(Play),
    StopAll,
}

/// Plays sound files on a thread of its own: audio outputs cannot move between threads.
#[derive(Clone)]
pub struct SoundPlayer {
    requests: Sender<Request>,
}

impl SoundPlayer {
    pub fn start() -> Self {
        let (requests, incoming) = mpsc::channel();
        thread::Builder::new()
            .name("sound".into())
            .spawn(move || Worker::default().run(incoming))
            .expect("spawn sound player");
        Self { requests }
    }

    pub fn play(&self, play: Play) {
        let _ = self.requests.send(Request::Play(play));
    }

    pub fn stop_all(&self) {
        let _ = self.requests.send(Request::StopAll);
    }
}

/// Names of the audio outputs, for the editor to offer.
pub fn output_devices() -> Vec<String> {
    let Ok(devices) = rodio::cpal::default_host().output_devices() else {
        return Vec::new();
    };
    devices
        .filter_map(|device| device.description().ok())
        .map(|description| description.name().to_owned())
        .collect()
}

#[derive(Default)]
struct Worker {
    /// One per output in use, opened on first use. `None` is the system default.
    outputs: HashMap<Option<String>, MixerDeviceSink>,
    playing: HashMap<String, Vec<Player>>,
}

impl Worker {
    fn run(mut self, incoming: Receiver<Request>) {
        for request in incoming {
            match request {
                Request::Play(play) => {
                    if let Err(err) = self.play(play) {
                        warn!("could not play a sound: {err}");
                    }
                }
                Request::StopAll => {
                    for player in self.playing.drain().flat_map(|(_, players)| players) {
                        player.stop();
                    }
                }
            }
        }
    }

    fn play(&mut self, play: Play) -> Result<(), String> {
        for players in self.playing.values_mut() {
            players.retain(|player| !player.empty());
        }
        let running = self.playing.get(&play.id).is_some_and(|p| !p.is_empty());
        match play.mode {
            SoundMode::Overlap => {}
            SoundMode::Restart => self.stop(&play.id),
            SoundMode::Toggle | SoundMode::Loop if running => {
                self.stop(&play.id);
                return Ok(());
            }
            SoundMode::Toggle | SoundMode::Loop => {}
        }
        let file = File::open(&play.path).map_err(|err| err.to_string())?;
        let decoder = Decoder::try_from(BufReader::new(file)).map_err(|err| err.to_string())?;
        let player = Player::connect_new(self.output(play.device)?.mixer());
        player.set_volume(play.volume.clamp(0.0, 1.0));
        if play.mode == SoundMode::Loop {
            player.append(decoder.repeat_infinite());
        } else {
            player.append(decoder);
        }
        self.playing.entry(play.id).or_default().push(player);
        Ok(())
    }

    fn stop(&mut self, id: &str) {
        for player in self.playing.remove(id).unwrap_or_default() {
            player.stop();
        }
    }

    /// The named output if it is still there, otherwise the system default.
    fn output(&mut self, device: Option<String>) -> Result<&MixerDeviceSink, String> {
        let found = device.filter(|name| output_devices().contains(name));
        if !self.outputs.contains_key(&found) {
            let mut sink = open(found.as_deref()).map_err(|err| err.to_string())?;
            sink.log_on_drop(false);
            self.outputs.insert(found.clone(), sink);
        }
        Ok(&self.outputs[&found])
    }
}

fn open(name: Option<&str>) -> Result<MixerDeviceSink, rodio::DeviceSinkError> {
    let device = name.and_then(|name| {
        rodio::cpal::default_host()
            .output_devices()
            .ok()?
            .find(|d| d.description().is_ok_and(|desc| desc.name() == name))
    });
    match device {
        Some(device) => DeviceSinkBuilder::from_device(device)?.open_stream(),
        None => DeviceSinkBuilder::open_default_sink(),
    }
}

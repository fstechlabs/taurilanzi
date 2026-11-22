use std::collections::{BTreeMap, BTreeSet};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::{Duration, Instant};

use hidapi::HidDevice;
use log::{debug, warn};
use serde::Deserialize;
use serde_json::json;

use super::page_archive::{self, Scope};
use super::protocol::{self, Command, inbound};
use super::{KEY_COUNT, WIDE_KEY};
use crate::driver::device::DeviceInfo;
use crate::driver::dock::{Event, Reporter, Request};

const READ_TIMEOUT_MS: i32 = 20;
const BUSY_READ_TIMEOUT_MS: i32 = 2;
const BUSY_FOR: Duration = Duration::from_millis(500);
/// Firmware 5.0.9 shows its own screensaver after about 10 s without hearing from the host.
const KEEP_ALIVE_INTERVAL: Duration = Duration::from_secs(5);
/// Mode 2 makes the firmware show only our image on the wide key, without its clock or stats.
const WIDE_KEY_BACKGROUND: &[u8] = b"2|0|0|00:00:00|0";

#[derive(Default)]
struct DesiredState {
    brightness: Option<u8>,
    sleep_minutes: Option<u32>,
    images: BTreeMap<u8, Vec<u8>>,
    screensaver: Option<Vec<u8>>,
}

#[derive(Default)]
struct Pending {
    brightness: bool,
    sleep: bool,
    screensaver: bool,
    full_page: bool,
    keys: BTreeSet<u8>,
}

/// Talks to one connected D200 until it goes, or until the engine drops its handle.
pub(in crate::driver) fn run(device: &HidDevice, requests: Receiver<Request>, reporter: &Reporter) {
    Worker {
        requests,
        events: reporter,
        state: DesiredState::default(),
        pending: Pending::default(),
        upload_id: 0,
        announced: false,
    }
    .session(device);
}

struct Worker<'a> {
    requests: Receiver<Request>,
    events: &'a Reporter,
    state: DesiredState,
    pending: Pending,
    upload_id: u64,
    announced: bool,
}

impl Worker<'_> {
    fn session(&mut self, device: &HidDevice) {
        let mut buf = [0u8; protocol::REPORT_SIZE];
        let mut last_keep_alive = Instant::now();
        let mut last_request = Instant::now();

        let opening = write(device, Command::ScreenUnlocked, &[])
            .and_then(|_| write(device, Command::LabelStyle, &label_style()))
            .and_then(|_| write(device, Command::DeviceInfo, &[]));
        if let Err(err) = opening {
            warn!("d200 handshake failed: {err}");
            return;
        }

        loop {
            // While images keep coming (an animation), waiting to read would hold the next one.
            let timeout = if last_request.elapsed() < BUSY_FOR {
                BUSY_READ_TIMEOUT_MS
            } else {
                READ_TIMEOUT_MS
            };
            match device.read_timeout(&mut buf, timeout) {
                Ok(0) => {}
                Ok(n) => self.handle_inbound(&buf[..n]),
                Err(err) => {
                    warn!("d200 read failed: {err}");
                    return;
                }
            }

            loop {
                match self.requests.try_recv() {
                    Ok(request) => {
                        last_request = Instant::now();
                        self.apply(request);
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => return,
                }
            }

            if last_keep_alive.elapsed() >= KEEP_ALIVE_INTERVAL {
                if let Err(err) = self.keep_alive(device) {
                    warn!("d200 keep-alive failed: {err}");
                    return;
                }
                last_keep_alive = Instant::now();
            }

            if let Err(err) = self.flush(device) {
                warn!("d200 write failed: {err}");
                return;
            }
        }
    }

    fn apply(&mut self, request: Request) {
        match request {
            Request::Brightness(percent) => {
                self.state.brightness = Some(percent);
                self.pending.brightness = true;
            }
            Request::SleepMinutes(minutes) => {
                self.state.sleep_minutes = Some(minutes);
                self.pending.sleep = true;
            }
            Request::SetKeys(images) => {
                // Live keys are drawn again on every change they might show; the dock only
                // gets the ones that look different.
                for (index, png) in images {
                    if self.state.images.get(&index) != Some(&png) {
                        self.pending.keys.insert(index);
                        self.state.images.insert(index, png);
                    }
                }
            }
            Request::ReplacePage(images) => {
                self.state.images = images;
                self.pending.full_page = true;
                self.pending.keys.clear();
            }
            Request::Screensaver(png) => {
                self.state.screensaver = Some(png);
                self.pending.screensaver = true;
            }
        }
    }

    fn flush(&mut self, device: &HidDevice) -> anyhow::Result<()> {
        if std::mem::take(&mut self.pending.brightness)
            && let Some(percent) = self.state.brightness
        {
            write(device, Command::Brightness, percent.to_string().as_bytes())?;
        }
        if std::mem::take(&mut self.pending.sleep)
            && let Some(minutes) = self.state.sleep_minutes
        {
            write(
                device,
                Command::SleepMinutes,
                minutes.to_string().as_bytes(),
            )?;
        }
        if std::mem::take(&mut self.pending.screensaver)
            && let Some(png) = &self.state.screensaver
        {
            let zip = page_archive::screensaver(png)?;
            debug!("d200 screensaver, {} bytes", zip.len());
            write(device, Command::Screensaver, &zip)?;
        }
        if std::mem::take(&mut self.pending.full_page) {
            self.pending.keys.clear();
            let upload_id = self.next_upload_id();
            let zip = page_archive::build(&self.state.images, Scope::Full, upload_id)?;
            debug!("d200 full page, {} bytes", zip.len());
            write(device, Command::FullPage, &zip)?;
            write(device, Command::WideKeyData, WIDE_KEY_BACKGROUND)?;
        } else if !self.pending.keys.is_empty() {
            let keys = std::mem::take(&mut self.pending.keys);
            let images: BTreeMap<u8, Vec<u8>> = keys
                .into_iter()
                .filter_map(|index| {
                    self.state
                        .images
                        .get(&index)
                        .map(|png| (index, png.clone()))
                })
                .collect();
            if !images.is_empty() {
                let zip = page_archive::build(&images, Scope::Partial, self.next_upload_id())?;
                debug!(
                    "d200 partial page, {} keys, {} bytes",
                    images.len(),
                    zip.len()
                );
                write(device, Command::PartialPage, &zip)?;
            }
        }
        Ok(())
    }

    fn keep_alive(&self, device: &HidDevice) -> anyhow::Result<()> {
        write(device, Command::ScreenUnlocked, &[])?;
        write(device, Command::WideKeyData, WIDE_KEY_BACKGROUND)
    }

    fn handle_inbound(&mut self, report: &[u8]) {
        let Some(message) = protocol::decode(report) else {
            return;
        };
        match message.command {
            inbound::KEY if message.payload.len() >= 4 => {
                let index = message.payload[1];
                let pressed = message.payload[3] == 1;
                // Docks of the same id with more controls, as the D200X with its dials, report
                // them here too: what is not a key of the D200 is not read as one.
                if index < KEY_COUNT || index == WIDE_KEY {
                    self.events.send(Event::Key { index, pressed });
                } else {
                    debug!("ignored d200 input {index}");
                }
            }
            // The dock may answer the info request more than once per connection.
            inbound::DEVICE_INFO if !self.announced => match parse_device_info(&message.payload) {
                Some(device_info) => {
                    self.announced = true;
                    self.events.send(Event::Connected(device_info));
                }
                None => warn!("unreadable d200 device info"),
            },
            inbound::DEVICE_INFO => {}
            other => debug!("ignored d200 message {other:#06x}"),
        }
    }

    fn next_upload_id(&mut self) -> u64 {
        self.upload_id += 1;
        self.upload_id
    }
}

fn write(device: &HidDevice, command: Command, payload: &[u8]) -> anyhow::Result<()> {
    for report in protocol::encode(command, payload) {
        device.write(&report)?;
    }
    Ok(())
}

fn label_style() -> Vec<u8> {
    serde_json::to_vec(&json!({
        "Align": "bottom",
        "Color": 0xffffff,
        "FontName": "Source Han Sans SC",
        "ShowTitle": false,
        "Size": 10,
        "Weight": 80,
    }))
    .expect("label style serializes")
}

fn parse_device_info(payload: &[u8]) -> Option<DeviceInfo> {
    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct Raw {
        serial_number: String,
        device_type: String,
        #[serde(rename = "Dversion")]
        firmware: String,
        hardware_version: String,
    }
    let text = payload.split(|&b| b == 0).next()?;
    let raw: Raw = serde_json::from_slice(text).ok()?;
    Some(DeviceInfo {
        serial: raw.serial_number,
        model: raw.device_type,
        firmware: raw.firmware,
        hardware: raw.hardware_version,
    })
}

#[cfg(test)]
mod tests {
    use hidapi::HidApi;

    use super::*;
    use crate::driver::ulanzi::{DATA_INTERFACE, PRODUCT_ID, VENDOR_ID};

    fn open(api: &mut HidApi) -> Option<HidDevice> {
        api.refresh_devices().ok()?;
        api.device_list()
            .find(|d| {
                d.vendor_id() == VENDOR_ID
                    && d.product_id() == PRODUCT_ID
                    && d.interface_number() == DATA_INTERFACE
            })?
            .open_device(api)
            .ok()
    }

    /// How fast the dock takes key images, to size animations. Needs the real dock, free of
    /// Ulanzi Studio and the app: `cargo test --release --lib frame_rate -- --ignored --nocapture`
    #[test]
    #[ignore = "needs the real dock"]
    fn frame_rate() {
        use crate::render::{Background, Color, KeyDesign, Layer, Renderer, VerticalPosition};

        let mut api = HidApi::new().unwrap();
        let device =
            open(&mut api).expect("no dock: connect it and close Ulanzi Studio and the app");
        write(&device, Command::ScreenUnlocked, &[]).unwrap();
        write(&device, Command::LabelStyle, &label_style()).unwrap();
        let mut renderer = Renderer::new();
        let frames: Vec<Vec<u8>> = (0..48)
            .map(|i| {
                let t = (i as f32 / 47.0 * std::f32::consts::PI).sin();
                let design = KeyDesign {
                    background: Background::LinearGradient {
                        from: Color::rgb(255, 0, 102),
                        to: Color::rgb(51, 0, 204),
                        angle: 90.0 + 180.0 * t,
                    },
                    layers: vec![Layer::Text {
                        content: format!("{i}"),
                        font_family: "Inter".into(),
                        font_weight: 700,
                        size: 60.0,
                        color: Color::WHITE,
                        position: VerticalPosition::Middle,
                        outline: None,
                    }],
                };
                let started = Instant::now();
                let png = renderer.render(&design, super::super::KEY_SIZE).unwrap();
                if i == 0 {
                    println!("render: {:?}, {} bytes", started.elapsed(), png.len());
                }
                png.as_ref().clone()
            })
            .collect();
        let mut buf = [0u8; protocol::REPORT_SIZE];
        for keys in [1usize, 3, 13] {
            let mut times = Vec::new();
            let started = Instant::now();
            for (n, frame) in frames.iter().enumerate() {
                let images: BTreeMap<u8, Vec<u8>> =
                    (0..keys as u8).map(|k| (k, frame.clone())).collect();
                let zip = page_archive::build(&images, Scope::Partial, n as u64 + 1).unwrap();
                if n == 0 {
                    println!(
                        "zip {} bytes, {} reports",
                        zip.len(),
                        protocol::encode(Command::PartialPage, &zip).len()
                    );
                }
                let at = Instant::now();
                write(&device, Command::PartialPage, &zip).unwrap();
                times.push(at.elapsed());
                while device.read_timeout(&mut buf, 0).unwrap_or(0) > 0 {}
            }
            let total = started.elapsed();
            times.sort();
            println!(
                "{keys} keys: {} frames in {total:?} ({:.1} fps), write median {:?}, max {:?}",
                frames.len(),
                frames.len() as f64 / total.as_secs_f64(),
                times[times.len() / 2],
                times[times.len() - 1],
            );
        }
        write(
            &device,
            Command::FullPage,
            &page_archive::build(&BTreeMap::new(), Scope::Full, 999).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn parses_device_info_with_padding() {
        let mut payload = br#"{"SerialNumber":"TEST","Dversion":"5.3.6","error":"0","DeviceType":"D200","HardwareVersion":"SSD210V100"}"#.to_vec();
        payload.resize(1014, 0);
        let parsed = parse_device_info(&payload).unwrap();
        assert_eq!(parsed.model, "D200");
        assert_eq!(parsed.firmware, "5.3.6");
        assert_eq!(parsed.serial, "TEST");
    }
}

use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{
    EDataFlow, IMMDeviceEnumerator, MMDeviceEnumerator, eCapture, eConsole, eRender,
};
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance};

use crate::monitor::feeds::Volume;
use crate::os::windows::multithreaded;

/// The volume of the default speakers and microphone, as the sound settings show them.
pub fn volume() -> Option<Volume> {
    multithreaded();
    // SAFETY: COM is initialised on this thread.
    let devices: IMMDeviceEnumerator =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }.ok()?;
    let output = level(&devices, eRender);
    let input = level(&devices, eCapture);
    (output.is_some() || input.is_some()).then(|| Volume {
        output: output.map(|(level, _)| level),
        input: input.map(|(level, _)| level),
        muted: output.is_some_and(|(_, muted)| muted),
    })
}

fn level(devices: &IMMDeviceEnumerator, flow: EDataFlow) -> Option<(u8, bool)> {
    // SAFETY: interfaces COM just returned, used on the thread that got them.
    unsafe {
        let device = devices.GetDefaultAudioEndpoint(flow, eConsole).ok()?;
        let volume: IAudioEndpointVolume = device.Activate(CLSCTX_ALL, None).ok()?;
        let level = volume.GetMasterVolumeLevelScalar().ok()?;
        let muted = volume.GetMute().ok()?.as_bool();
        Some(((level * 100.0).round().clamp(0.0, 100.0) as u8, muted))
    }
}

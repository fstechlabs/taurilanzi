use std::fs;
use std::path::Path;
use std::process::Command;

/// How busy the busiest GPU is: AMD and Intel drivers publish it in sysfs; NVIDIA's only through
/// its own tool.
pub fn gpu_utilization() -> Option<f32> {
    let from_sysfs = fs::read_dir("/sys/class/drm")
        .ok()?
        .flatten()
        .filter_map(|card| fs::read_to_string(card.path().join("device/gpu_busy_percent")).ok())
        .filter_map(|text| text.trim().parse::<f32>().ok())
        .reduce(f32::max);
    from_sysfs
        .or_else(nvidia)
        .map(|percent| (percent / 100.0).clamp(0.0, 1.0))
}

fn nvidia() -> Option<f32> {
    if !Path::new("/proc/driver/nvidia").exists() {
        return None;
    }
    let output = Command::new("nvidia-smi")
        .args([
            "--query-gpu=utilization.gpu",
            "--format=csv,noheader,nounits",
        ])
        .output()
        .ok()?;
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.trim().parse::<f32>().ok())
        .reduce(f32::max)
}

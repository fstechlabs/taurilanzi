/// Not read yet on this system.
pub fn gpu_utilization() -> Option<f32> {
    None
}

pub fn battery() -> Option<super::feeds::Battery> {
    None
}

pub fn now_playing() -> Option<super::feeds::Track> {
    None
}

pub fn volume() -> Option<super::feeds::Volume> {
    None
}

pub fn router() -> Option<String> {
    None
}

pub fn ping(_host: &str) -> Option<f32> {
    None
}

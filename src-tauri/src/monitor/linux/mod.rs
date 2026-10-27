mod audio;
mod gpu;
mod network;
mod now_playing;
mod power;

pub use audio::volume;
pub use gpu::gpu_utilization;
pub use network::{ping, router};
pub use now_playing::now_playing;
pub use power::battery;

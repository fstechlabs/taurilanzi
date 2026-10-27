mod apps;
mod backend;
mod gnome;
mod kwin;
mod wlroots;
mod x11;

pub use apps::{app_at, app_icon, running_apps};
pub use backend::{detection, frontmost, set_up_detection};

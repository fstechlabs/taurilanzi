mod catalog;
mod device;
mod dock;
mod model;
pub mod ulanzi;
mod virtual_dock;
mod watcher;

pub use catalog::{FIRST_MODEL, model, models};
pub use device::DeviceInfo;
pub use dock::{ConnectionId, Dock, DockEvent, Event};
pub use model::{ControlKind, DeviceModel, Size};
#[cfg(test)]
pub use virtual_dock::plugged;
pub use virtual_dock::{parse as virtual_models, start as start_virtual};
pub use watcher::watch;

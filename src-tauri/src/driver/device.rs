use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct DeviceInfo {
    pub serial: String,
    pub model: String,
    pub firmware: String,
    pub hardware: String,
}

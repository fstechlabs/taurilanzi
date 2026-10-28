//! The plugins the computers of an account have installed, so each of them can install the
//! others' too.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnownPlugin {
    pub name: String,
    /// Its package in the community catalog; none when it was installed from a file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog: Option<String>,
}

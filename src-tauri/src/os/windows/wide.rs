use std::ffi::{OsStr, OsString};
use std::os::windows::ffi::{OsStrExt, OsStringExt};

/// A null-terminated UTF-16 string, as Windows takes them.
pub fn to_wide(text: impl AsRef<OsStr>) -> Vec<u16> {
    text.as_ref().encode_wide().chain(Some(0)).collect()
}

/// Up to the first null, if any.
pub fn from_wide(units: &[u16]) -> OsString {
    let end = units.iter().position(|&u| u == 0).unwrap_or(units.len());
    OsString::from_wide(&units[..end])
}

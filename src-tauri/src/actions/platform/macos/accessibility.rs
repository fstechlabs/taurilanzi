use std::ffi::c_void;

use objc2_foundation::{NSDictionary, NSNumber, NSString};

use crate::actions::error::ActionError;

const SETTINGS: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility";

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;
}

/// Whether macOS lets the app send keystrokes. Without it they are silently dropped.
pub fn accessibility_granted() -> bool {
    // SAFETY: no arguments and no preconditions.
    unsafe { AXIsProcessTrusted() }
}

/// Shows the system prompt, which also adds the app to the list in System Settings.
pub fn request_accessibility() {
    let key = NSString::from_str("AXTrustedCheckOptionPrompt");
    let options = NSDictionary::from_slices(&[&*key], &[&*NSNumber::new_bool(true)]);
    // SAFETY: an NSDictionary is toll-free bridged to the CFDictionary the function expects.
    unsafe { AXIsProcessTrustedWithOptions(&*options as *const _ as *const c_void) };
}

pub fn open_accessibility_settings() -> Result<(), ActionError> {
    super::apps::open(SETTINGS)
}

pub(super) fn require() -> Result<(), ActionError> {
    if accessibility_granted() {
        Ok(())
    } else {
        Err(ActionError::NoAccessibility)
    }
}

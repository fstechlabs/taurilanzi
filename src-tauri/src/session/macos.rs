use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_core_foundation::CFRetained;
use objc2_core_graphics::CGSessionCopyCurrentDictionary;
use objc2_foundation::{NSDictionary, NSNumber, NSString};

/// The key macOS adds to the session while the lock screen is up, and removes on unlocking.
const LOCKED_KEY: &str = "CGSSessionScreenIsLocked";

pub fn screen_locked() -> Option<bool> {
    let session = CGSessionCopyCurrentDictionary()?;
    // SAFETY: CFDictionary is toll-free bridged with NSDictionary, and `session` keeps it alive
    // for as long as the reference is used.
    let session: &NSDictionary<NSString, AnyObject> =
        unsafe { &*CFRetained::as_ptr(&session).as_ptr().cast() };
    let Some(value) = session.objectForKey(&NSString::from_str(LOCKED_KEY)) else {
        return Some(false);
    };
    Some(
        Retained::downcast::<NSNumber>(value)
            .map(|number| number.boolValue())
            .unwrap_or(false),
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn reads_the_session() {
        assert!(super::screen_locked().is_some());
    }
}

use std::path::Path;
use std::process::Command;

use objc2_app_kit::NSRunningApplication;
use objc2_foundation::{NSBundle, NSString};

use crate::actions::error::ActionError;

/// Opens an app, a file, a folder or a URL, as Finder would.
pub fn open(target: &str) -> Result<(), ActionError> {
    let status = Command::new("/usr/bin/open").arg(target).status()?;
    if status.success() {
        Ok(())
    } else {
        Err(ActionError::Launch(std::io::Error::other(format!(
            "open exited with {status}"
        ))))
    }
}

/// Asks every running copy of the app to quit, as Quit in its menu would; unsaved work is
/// still offered to be saved.
pub fn close_app(app: &str) -> Result<(), ActionError> {
    if !Path::new(app).exists() {
        return Err(ActionError::NotFound(app.into()));
    }
    let bundle = NSBundle::bundleWithPath(&NSString::from_str(app))
        .and_then(|bundle| bundle.bundleIdentifier())
        .ok_or_else(|| ActionError::NotAnApp(app.into()))?;
    for running in NSRunningApplication::runningApplicationsWithBundleIdentifier(&bundle) {
        running.terminate();
    }
    Ok(())
}

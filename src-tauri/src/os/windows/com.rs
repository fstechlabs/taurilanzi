//! COM, which the shell, the audio devices and the media session need on the thread that calls
//! them. Once a thread has it, it keeps it until the thread ends.

use std::cell::Cell;

use windows::Win32::System::Com::{
    COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, COINIT_MULTITHREADED, CoInitializeEx,
};

thread_local! {
    static READY: Cell<bool> = const { Cell::new(false) };
}

/// For shell calls (opening files, reading shortcuts), which expect a single-threaded apartment.
pub fn apartment() {
    initialize(COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);
}

/// For threads that wait on system calls without a message loop, such as the monitor's.
pub fn multithreaded() {
    initialize(COINIT_MULTITHREADED);
}

/// A thread already set up the other way keeps that apartment, where COM works all the same.
fn initialize(mode: windows::Win32::System::Com::COINIT) {
    READY.with(|ready| {
        if !ready.get() {
            // SAFETY: no reserved argument; a failure only means COM was already set up.
            let _ = unsafe { CoInitializeEx(None, mode) };
            ready.set(true);
        }
    });
}

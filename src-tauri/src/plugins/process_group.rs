//! On macOS and Linux a plugin may start programs of its own, which outlive it when only the
//! plugin is ended. Each plugin leads a process group of its own, which is ended whole.

use std::os::unix::process::CommandExt;
use std::process::Command;

pub fn isolate(command: &mut Command) {
    command.process_group(0);
}

/// Asks every process of the group a plugin leads to end, or makes them.
pub fn end(leader: u32, force: bool) {
    let Ok(group) = libc::pid_t::try_from(leader) else {
        return;
    };
    let signal = if force { libc::SIGKILL } else { libc::SIGTERM };
    // SAFETY: killpg only sends a signal; a group that no longer exists makes it fail harmlessly.
    unsafe {
        libc::killpg(group, signal);
    }
}

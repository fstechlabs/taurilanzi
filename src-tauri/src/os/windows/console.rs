use std::os::windows::process::CommandExt;
use std::process::Command;

/// Keeps a console program from opening a console window of its own.
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn without_console(command: &mut Command) -> &mut Command {
    command.creation_flags(CREATE_NO_WINDOW)
}

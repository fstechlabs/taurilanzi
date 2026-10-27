use std::io;
use std::path::Path;

pub fn register(_label: &str, _name: &str, _app: &Path, _background_arg: &str) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "starting with the session is not supported on this system",
    ))
}

use std::io;

fn unsupported() -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        "no system keychain is supported on this system",
    )
}

pub fn get(_service: &str, _account: &str) -> io::Result<Option<String>> {
    Err(unsupported())
}

pub fn exists(_service: &str, _account: &str) -> io::Result<bool> {
    Err(unsupported())
}

pub fn set(_service: &str, _account: &str, _value: Option<&str>) -> io::Result<()> {
    Err(unsupported())
}

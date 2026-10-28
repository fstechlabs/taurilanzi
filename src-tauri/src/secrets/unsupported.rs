use std::io;

pub fn get(_service: &str, _account: &str) -> io::Result<Option<String>> {
    Ok(None)
}

pub fn exists(_service: &str, _account: &str) -> io::Result<bool> {
    Ok(false)
}

pub fn delete(_service: &str, _account: &str) -> io::Result<()> {
    Ok(())
}

//! Passwords in the desktop's keyring (GNOME Keyring, KWallet), through the Secret Service, where
//! earlier versions kept the secrets.

use std::collections::HashMap;
use std::io;

use secret_service::EncryptionType;
use secret_service::blocking::SecretService;

fn attributes<'a>(service: &'a str, account: &'a str) -> HashMap<&'a str, &'a str> {
    HashMap::from([("service", service), ("account", account)])
}

fn failed(err: secret_service::Error) -> io::Error {
    io::Error::other(err.to_string())
}

pub fn get(service: &str, account: &str) -> io::Result<Option<String>> {
    let keyring = SecretService::connect(EncryptionType::Dh).map_err(failed)?;
    let found = keyring
        .search_items(attributes(service, account))
        .map_err(failed)?;
    let Some(item) = found.unlocked.first().or(found.locked.first()) else {
        return Ok(None);
    };
    item.ensure_unlocked().map_err(failed)?;
    let secret = item.get_secret().map_err(failed)?;
    String::from_utf8(secret)
        .map(Some)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
}

pub fn exists(service: &str, account: &str) -> io::Result<bool> {
    let keyring = SecretService::connect(EncryptionType::Dh).map_err(failed)?;
    let found = keyring
        .search_items(attributes(service, account))
        .map_err(failed)?;
    Ok(!found.unlocked.is_empty() || !found.locked.is_empty())
}

pub fn delete(service: &str, account: &str) -> io::Result<()> {
    let keyring = SecretService::connect(EncryptionType::Dh).map_err(failed)?;
    let found = keyring
        .search_items(attributes(service, account))
        .map_err(failed)?;
    for item in found.unlocked.iter().chain(&found.locked) {
        item.delete().map_err(failed)?;
    }
    Ok(())
}

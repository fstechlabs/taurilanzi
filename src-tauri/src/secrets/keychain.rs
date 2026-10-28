//! Generic passwords in the macOS keychain, where earlier versions kept the secrets.

use std::io;

use security_framework::item::{ItemClass, ItemSearchOptions};
use security_framework::passwords;

/// `errSecItemNotFound`.
const NOT_FOUND: i32 = -25300;

pub fn get(service: &str, account: &str) -> io::Result<Option<String>> {
    match passwords::get_generic_password(service, account) {
        Ok(bytes) => String::from_utf8(bytes)
            .map(Some)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err)),
        Err(err) if err.code() == NOT_FOUND => Ok(None),
        Err(err) => Err(io::Error::other(err)),
    }
}

/// Without reading the secret, so the keychain does not ask for permission.
pub fn exists(service: &str, account: &str) -> io::Result<bool> {
    let found = ItemSearchOptions::new()
        .class(ItemClass::generic_password())
        .service(service)
        .account(account)
        .load_attributes(true)
        .search();
    match found {
        Ok(items) => Ok(!items.is_empty()),
        Err(err) if err.code() == NOT_FOUND => Ok(false),
        Err(err) => Err(io::Error::other(err)),
    }
}

pub fn delete(service: &str, account: &str) -> io::Result<()> {
    match passwords::delete_generic_password(service, account) {
        Err(err) if err.code() == NOT_FOUND => Ok(()),
        other => other.map_err(io::Error::other),
    }
}

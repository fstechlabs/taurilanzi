//! Generic credentials in the Windows Credential Manager, where earlier versions kept the secrets.

use std::io;

use windows::Win32::Foundation::{ERROR_NOT_FOUND, WIN32_ERROR};
use windows::Win32::Security::Credentials::{
    CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredFree, CredReadW,
};
use windows::core::HSTRING;

fn target(service: &str, account: &str) -> HSTRING {
    HSTRING::from(format!("{service}/{account}"))
}

fn not_found(err: &windows::core::Error) -> bool {
    err.code() == WIN32_ERROR(ERROR_NOT_FOUND.0).to_hresult()
}

pub fn get(service: &str, account: &str) -> io::Result<Option<String>> {
    let mut credential: *mut CREDENTIALW = std::ptr::null_mut();
    // SAFETY: the system allocates the credential, which is freed below.
    unsafe {
        match CredReadW(
            &target(service, account),
            CRED_TYPE_GENERIC,
            None,
            &mut credential,
        ) {
            Ok(()) => {}
            Err(err) if not_found(&err) => return Ok(None),
            Err(err) => return Err(io::Error::other(err)),
        }
        let blob = std::slice::from_raw_parts(
            (*credential).CredentialBlob,
            (*credential).CredentialBlobSize as usize,
        )
        .to_vec();
        CredFree(credential.cast());
        String::from_utf8(blob)
            .map(Some)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
    }
}

pub fn exists(service: &str, account: &str) -> io::Result<bool> {
    get(service, account).map(|value| value.is_some())
}

pub fn delete(service: &str, account: &str) -> io::Result<()> {
    // SAFETY: a plain call with a string that outlives it.
    match unsafe { CredDeleteW(&target(service, account), CRED_TYPE_GENERIC, None) } {
        Err(err) if not_found(&err) => Ok(()),
        other => other.map_err(io::Error::other),
    }
}

//! Generic credentials in the Windows Credential Manager, one per service and account.

use std::io;

use windows::Win32::Foundation::{ERROR_NOT_FOUND, WIN32_ERROR};
use windows::Win32::Security::Credentials::{
    CRED_FLAGS, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredFree,
    CredReadW, CredWriteW,
};
use windows::core::{HSTRING, PWSTR};

use crate::os::windows::to_wide;

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

pub fn set(service: &str, account: &str, value: Option<&str>) -> io::Result<()> {
    let target = target(service, account);
    match value {
        Some(value) => {
            let mut name = to_wide(format!("{service}/{account}"));
            let mut user = to_wide(account);
            let mut blob = value.as_bytes().to_vec();
            let credential = CREDENTIALW {
                Flags: CRED_FLAGS(0),
                Type: CRED_TYPE_GENERIC,
                TargetName: PWSTR(name.as_mut_ptr()),
                CredentialBlobSize: blob.len() as u32,
                CredentialBlob: blob.as_mut_ptr(),
                Persist: CRED_PERSIST_LOCAL_MACHINE,
                UserName: PWSTR(user.as_mut_ptr()),
                ..Default::default()
            };
            // SAFETY: every pointer in the credential outlives the call.
            unsafe { CredWriteW(&credential, 0) }.map_err(io::Error::other)
        }
        // SAFETY: a plain call with a string that outlives it.
        None => match unsafe { CredDeleteW(&target, CRED_TYPE_GENERIC, None) } {
            Err(err) if not_found(&err) => Ok(()),
            other => other.map_err(io::Error::other),
        },
    }
}

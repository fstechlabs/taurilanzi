use std::net::{IpAddr, Ipv4Addr, ToSocketAddrs};

use windows::Win32::NetworkManagement::IpHelper::{
    FreeMibTable, GetIpForwardTable2, ICMP_ECHO_REPLY, IcmpCloseHandle, IcmpCreateFile,
    IcmpSendEcho, MIB_IPFORWARD_TABLE2,
};
use windows::Win32::Networking::WinSock::AF_INET;

use crate::monitor::feeds::safe_host;

const TIMEOUT_MS: u32 = 1000;

/// The router: the next hop of the default route with the lowest metric.
pub fn router() -> Option<String> {
    let mut table: *mut MIB_IPFORWARD_TABLE2 = std::ptr::null_mut();
    // SAFETY: the system allocates the table, which is freed below.
    unsafe {
        GetIpForwardTable2(AF_INET, &mut table).ok().ok()?;
        let rows =
            std::slice::from_raw_parts((*table).Table.as_ptr(), (*table).NumEntries as usize);
        let best = rows
            .iter()
            .filter(|row| row.DestinationPrefix.PrefixLength == 0)
            .filter_map(|row| {
                let hop = Ipv4Addr::from(row.NextHop.Ipv4.sin_addr.S_un.S_addr.to_ne_bytes());
                (!hop.is_unspecified()).then_some((row.Metric, hop))
            })
            .min_by_key(|(metric, _)| *metric)
            .map(|(_, hop)| hop.to_string());
        FreeMibTable(table.cast());
        best.filter(|host| safe_host(host))
    }
}

/// Milliseconds until `host` answers, waiting a second at most. Without a program in between,
/// whose output Windows translates.
pub fn ping(host: &str) -> Option<f32> {
    let address = (host, 0)
        .to_socket_addrs()
        .ok()?
        .find_map(|address| match address.ip() {
            IpAddr::V4(ip) => Some(ip),
            IpAddr::V6(_) => None,
        })?;
    let payload = [0u8; 32];
    let mut reply = vec![0u8; size_of::<ICMP_ECHO_REPLY>() + payload.len() + 8];
    // SAFETY: the handle is closed below, and the buffers outlive the call.
    unsafe {
        let handle = IcmpCreateFile().ok()?;
        let answers = IcmpSendEcho(
            handle,
            u32::from_ne_bytes(address.octets()),
            payload.as_ptr().cast(),
            payload.len() as u16,
            None,
            reply.as_mut_ptr().cast(),
            reply.len() as u32,
            TIMEOUT_MS,
        );
        let _ = IcmpCloseHandle(handle);
        if answers == 0 {
            return None;
        }
        let reply = &*(reply.as_ptr() as *const ICMP_ECHO_REPLY);
        (reply.Status == 0).then_some(reply.RoundTripTime as f32)
    }
}

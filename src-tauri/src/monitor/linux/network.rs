use std::fs;
use std::net::Ipv4Addr;
use std::process::Command;

use crate::monitor::feeds::{parse_ping, safe_host};

/// The kernel marks routes that go through a gateway.
const GATEWAY_FLAG: u32 = 0x2;

/// The router: the gateway of the default route.
pub fn router() -> Option<String> {
    parse_routes(&fs::read_to_string("/proc/net/route").ok()?).filter(|host| safe_host(host))
}

/// Milliseconds until `host` answers, waiting a second at most.
pub fn ping(host: &str) -> Option<f32> {
    let output = Command::new("ping")
        .args(["-n", "-c", "1", "-W", "1", host])
        .output()
        .ok()?;
    parse_ping(&String::from_utf8_lossy(&output.stdout))
}

/// Interface, destination, gateway and flags lead each line, the addresses in hex as the kernel
/// stores them.
fn parse_routes(table: &str) -> Option<String> {
    table.lines().skip(1).find_map(|line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        let (destination, gateway, flags) = (fields.get(1)?, fields.get(2)?, fields.get(3)?);
        let flags = u32::from_str_radix(flags, 16).ok()?;
        if *destination != "00000000" || flags & GATEWAY_FLAG == 0 {
            return None;
        }
        let gateway = u32::from_str_radix(gateway, 16).ok()?;
        Some(Ipv4Addr::from(gateway.to_le_bytes()).to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_default_gateway() {
        let table = "Iface\tDestination\tGateway \tFlags\tRefCnt\tUse\tMetric\tMask\n\
            wlp2s0\t0000A8C0\t00000000\t0001\t0\t0\t600\t00FFFFFF\n\
            wlp2s0\t00000000\t0100A8C0\t0003\t0\t0\t600\t00000000\n";
        assert_eq!(parse_routes(table).as_deref(), Some("192.168.0.1"));
        assert_eq!(parse_routes("Iface\tDestination\n"), None);
    }
}

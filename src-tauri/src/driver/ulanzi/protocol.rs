pub const REPORT_SIZE: usize = 1024;
pub const HEADER_SIZE: usize = 8;
const MAGIC: [u8; 2] = [0x7c, 0x7c];
const TRAILER_SIZE: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum Command {
    FullPage = 0x0001,
    DeviceInfo = 0x0003,
    ScreenUnlocked = 0x0005,
    WideKeyData = 0x0006,
    Screensaver = 0x0009,
    Brightness = 0x000a,
    LabelStyle = 0x000b,
    PartialPage = 0x000d,
    SleepMinutes = 0x001a,
}

/// Builds the HID reports for one message, each prefixed with report ID 0.
///
/// Mirrors Ulanzi Studio: header + payload + two zero bytes, zero-padded up to the next
/// multiple of 1024, adding a full zero report when the size is already a multiple.
pub fn encode(command: Command, payload: &[u8]) -> Vec<[u8; REPORT_SIZE + 1]> {
    let content = HEADER_SIZE + payload.len() + TRAILER_SIZE;
    let total = content + (REPORT_SIZE - content % REPORT_SIZE);

    let mut buf = Vec::with_capacity(total);
    buf.extend_from_slice(&MAGIC);
    buf.extend_from_slice(&(command as u16).to_be_bytes());
    buf.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    buf.extend_from_slice(payload);
    buf.resize(total, 0);

    buf.chunks(REPORT_SIZE)
        .map(|chunk| {
            let mut report = [0u8; REPORT_SIZE + 1];
            report[1..].copy_from_slice(chunk);
            report
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inbound {
    pub command: u16,
    pub payload: Vec<u8>,
}

pub mod inbound {
    pub const KEY: u16 = 0x0101;
    pub const DEVICE_INFO: u16 = 0x0303;
}

/// Parses a report read from the device. The dock always sends single-report messages.
pub fn decode(report: &[u8]) -> Option<Inbound> {
    if report.len() < HEADER_SIZE || report[..2] != MAGIC {
        return None;
    }
    let command = u16::from_be_bytes([report[2], report[3]]);
    let declared = u32::from_le_bytes([report[4], report[5], report[6], report[7]]) as usize;
    let end = (HEADER_SIZE + declared).min(report.len());
    Some(Inbound {
        command,
        payload: report[HEADER_SIZE..end].to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_command_fits_in_one_report() {
        let reports = encode(Command::DeviceInfo, &[]);
        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0][0], 0, "report id");
        assert_eq!(&reports[0][1..9], &[0x7c, 0x7c, 0x00, 0x03, 0, 0, 0, 0]);
        assert!(reports[0][9..].iter().all(|&b| b == 0));
    }

    #[test]
    fn ascii_payload_and_little_endian_length() {
        let reports = encode(Command::Brightness, b"86");
        assert_eq!(
            &reports[0][1..11],
            &[0x7c, 0x7c, 0x00, 0x0a, 2, 0, 0, 0, b'8', b'6']
        );
    }

    #[test]
    fn large_payload_spans_reports_without_extra_headers() {
        let payload: Vec<u8> = (0..3000u32).map(|i| (i % 251) as u8 + 1).collect();
        let reports = encode(Command::FullPage, &payload);
        assert_eq!(reports.len(), 3);
        let joined: Vec<u8> = reports.iter().flat_map(|r| r[1..].to_vec()).collect();
        assert_eq!(&joined[8..8 + payload.len()], payload.as_slice());
        assert!(
            reports
                .iter()
                .all(|r| r.len() == REPORT_SIZE + 1 && r[0] == 0)
        );
    }

    #[test]
    fn exact_multiple_adds_a_zero_report() {
        let payload = vec![1u8; 2038];
        let reports = encode(Command::PartialPage, &payload);
        assert_eq!(reports.len(), 3);
        assert!(reports[2][1..].iter().all(|&b| b == 0));
    }

    #[test]
    fn decodes_key_event() {
        let mut report = [0u8; REPORT_SIZE];
        report[..12].copy_from_slice(&[
            0x7c, 0x7c, 0x01, 0x01, 0xf6, 0x03, 0, 0, 0x01, 0x07, 0x01, 0x01,
        ]);
        let msg = decode(&report).unwrap();
        assert_eq!(msg.command, inbound::KEY);
        assert_eq!(&msg.payload[..4], &[0x01, 0x07, 0x01, 0x01]);
    }

    #[test]
    fn rejects_reports_without_magic() {
        assert!(decode(&[0u8; REPORT_SIZE]).is_none());
    }
}

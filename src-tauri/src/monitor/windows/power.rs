use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};

use crate::monitor::feeds::{Battery, Power};

const NO_BATTERY: u8 = 128;
const CHARGING: u8 = 8;
const UNKNOWN: u8 = 255;

/// The battery as the system tray shows it; none on a desktop.
pub fn battery() -> Option<Battery> {
    let mut status = SYSTEM_POWER_STATUS::default();
    // SAFETY: `status` outlives the call.
    unsafe { GetSystemPowerStatus(&mut status) }.ok()?;
    from_status(&status)
}

fn from_status(status: &SYSTEM_POWER_STATUS) -> Option<Battery> {
    if status.BatteryFlag == UNKNOWN
        || status.BatteryFlag & NO_BATTERY != 0
        || status.BatteryLifePercent > 100
    {
        return None;
    }
    let power = if status.BatteryFlag & CHARGING != 0 {
        Power::Charging
    } else if status.ACLineStatus == 1 {
        Power::Plugged
    } else {
        Power::Draining
    };
    Some(Battery {
        percent: status.BatteryLifePercent,
        power,
        minutes_left: (power == Power::Draining && status.BatteryLifeTime != u32::MAX)
            .then_some(status.BatteryLifeTime / 60),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_power_status() {
        let draining = SYSTEM_POWER_STATUS {
            ACLineStatus: 0,
            BatteryFlag: 1,
            BatteryLifePercent: 64,
            BatteryLifeTime: 5400,
            ..Default::default()
        };
        assert_eq!(
            from_status(&draining),
            Some(Battery {
                percent: 64,
                power: Power::Draining,
                minutes_left: Some(90)
            })
        );
        let charging = SYSTEM_POWER_STATUS {
            ACLineStatus: 1,
            BatteryFlag: CHARGING,
            BatteryLifePercent: 30,
            BatteryLifeTime: u32::MAX,
            ..Default::default()
        };
        assert_eq!(from_status(&charging).unwrap().power, Power::Charging);
        let desktop = SYSTEM_POWER_STATUS {
            ACLineStatus: 1,
            BatteryFlag: NO_BATTERY,
            BatteryLifePercent: UNKNOWN,
            ..Default::default()
        };
        assert_eq!(from_status(&desktop), None);
    }
}

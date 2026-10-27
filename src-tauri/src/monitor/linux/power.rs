use std::fs;
use std::path::Path;

use crate::monitor::feeds::{Battery, Power};

const SUPPLIES: &str = "/sys/class/power_supply";

/// The computer's battery, from sysfs; none on a desktop. Batteries of mice and headsets are
/// supplies too, marked as belonging to a device.
pub fn battery() -> Option<Battery> {
    fs::read_dir(SUPPLIES).ok()?.flatten().find_map(|supply| {
        let read = |name: &str| read(&supply.path(), name);
        (read("type").as_deref() == Some("Battery") && read("scope").as_deref() != Some("Device"))
            .then(|| from_supply(&read))
            .flatten()
    })
}

fn read(supply: &Path, name: &str) -> Option<String> {
    fs::read_to_string(supply.join(name))
        .ok()
        .map(|text| text.trim().to_owned())
}

fn from_supply(read: &dyn Fn(&str) -> Option<String>) -> Option<Battery> {
    let percent: u8 = read("capacity")?.parse::<u16>().ok()?.min(100) as u8;
    let number = |name: &str| read(name).and_then(|v| v.parse::<f64>().ok());
    let power = match read("status").as_deref() {
        Some("Charging") => Power::Charging,
        Some("Discharging") => Power::Draining,
        _ => Power::Plugged,
    };
    // Drivers report energy and power, or charge and current; either gives the hours left.
    let (now, full, rate) = match (number("energy_now"), number("power_now")) {
        (Some(now), Some(rate)) => (now, number("energy_full"), rate),
        _ => (
            number("charge_now")?,
            number("charge_full"),
            number("current_now")?,
        ),
    };
    let hours = match power {
        _ if rate <= 0.0 => None,
        Power::Draining => Some(now / rate),
        Power::Charging => full.map(|full| (full - now).max(0.0) / rate),
        Power::Plugged => None,
    };
    Some(Battery {
        percent,
        power,
        minutes_left: hours.map(|h| (h * 60.0).round() as u32),
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn supply(values: &[(&str, &str)]) -> Option<Battery> {
        let values: HashMap<String, String> = values
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        from_supply(&|name| values.get(name).cloned())
    }

    #[test]
    fn reads_the_battery_from_sysfs() {
        assert_eq!(
            supply(&[
                ("capacity", "62"),
                ("status", "Discharging"),
                ("energy_now", "30000000"),
                ("energy_full", "50000000"),
                ("power_now", "10000000"),
            ]),
            Some(Battery {
                percent: 62,
                power: Power::Draining,
                minutes_left: Some(180)
            })
        );
        let charging = supply(&[
            ("capacity", "50"),
            ("status", "Charging"),
            ("charge_now", "2000000"),
            ("charge_full", "4000000"),
            ("current_now", "1000000"),
        ])
        .unwrap();
        assert_eq!(
            (charging.power, charging.minutes_left),
            (Power::Charging, Some(120))
        );
        let full = supply(&[
            ("capacity", "100"),
            ("status", "Full"),
            ("energy_now", "1"),
            ("power_now", "0"),
        ])
        .unwrap();
        assert_eq!((full.power, full.minutes_left), (Power::Plugged, None));
        assert_eq!(supply(&[("status", "Full")]), None);
    }
}

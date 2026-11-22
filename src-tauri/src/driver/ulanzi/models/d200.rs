use super::super::{
    COLUMNS, DATA_INTERFACE, KEY_COUNT, KEY_SIZE, PRODUCT_ID, VENDOR_ID, WIDE_KEY, WIDE_KEY_SIZE,
};
use crate::driver::model::{
    Cell, Control, ControlKind, DeviceModel, Family, PluginIdentity, Rect, Size, Usb, Variant,
};

/// Distance from one key to the next on the panel: a key and its gap.
const PITCH: u32 = 256;
const PANEL: Size = Size {
    width: 1220,
    height: 708,
};
/// The sleep times the firmware takes.
const SLEEP_MINUTES: [u32; 7] = [0, 1, 5, 10, 15, 30, 60];

pub fn d200() -> DeviceModel {
    let size = |(width, height): (u32, u32)| Size { width, height };
    let mut controls: Vec<Control> = (0..KEY_COUNT)
        .map(|index| {
            let (column, row) = (index % COLUMNS, index / COLUMNS);
            Control {
                index,
                kind: ControlKind::Key {
                    image: size(KEY_SIZE),
                },
                cell: Cell {
                    row,
                    column,
                    row_span: 1,
                    column_span: 1,
                },
                bounds: Rect {
                    x: u32::from(column) * PITCH,
                    y: u32::from(row) * PITCH,
                    width: KEY_SIZE.0,
                    height: KEY_SIZE.1,
                },
            }
        })
        .collect();
    controls.push(Control {
        index: WIDE_KEY,
        kind: ControlKind::Key {
            image: size(WIDE_KEY_SIZE),
        },
        cell: Cell {
            row: 2,
            column: 3,
            row_span: 1,
            column_span: 2,
        },
        bounds: Rect {
            x: 3 * PITCH,
            y: 2 * PITCH,
            width: WIDE_KEY_SIZE.0,
            height: WIDE_KEY_SIZE.1,
        },
    });
    DeviceModel {
        id: "ulanzi.d200",
        name: "Ulanzi D200",
        family: Family::Ulanzi,
        variants: vec![Variant {
            vendor: "Ulanzi",
            name: "D200",
            usb: Usb {
                vendor_id: VENDOR_ID,
                product_id: PRODUCT_ID,
                interface: DATA_INTERFACE,
            },
        }],
        panel: PANEL,
        columns: COLUMNS,
        rows: 3,
        controls,
        sleep_minutes: SLEEP_MINUTES.to_vec(),
        firmware_press_effect: true,
        screensaver: true,
        plugins: PluginIdentity {
            elgato_type: 0,
            ulanzi_model: Some("D200"),
        },
        stable_serial: true,
        confirmed_on_hardware: true,
    }
}

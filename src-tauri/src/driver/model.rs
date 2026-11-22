//! What a dock model is, for everything that is not its driver: the layout the editor draws, the
//! sizes keys are rendered at, the rules a configuration of it follows and how plugins see it.

use serde::Serialize;

/// One hardware layout. Rebrands of the same hardware are variants of one model.
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct DeviceModel {
    /// Kept in configurations: never renamed once released.
    pub id: &'static str,
    pub name: &'static str,
    pub family: Family,
    pub variants: Vec<Variant>,
    /// The face of the device, in the pixels its controls are laid out in.
    pub panel: Size,
    pub columns: u8,
    pub rows: u8,
    pub controls: Vec<Control>,
    /// Sleep times the firmware offers, in minutes; `0` never sleeps.
    pub sleep_minutes: Vec<u32>,
    /// The firmware shows a press by itself, so keys are drawn without one.
    pub firmware_press_effect: bool,
    /// The device keeps a picture of the whole panel to show while nobody talks to it.
    pub screensaver: bool,
    pub plugins: PluginIdentity,
    /// Whether the serial number tells two units apart; otherwise the USB port does.
    pub stable_serial: bool,
    pub confirmed_on_hardware: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum Family {
    Ulanzi,
}

/// A product sold with this hardware, and how it shows up on USB.
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct Variant {
    pub vendor: &'static str,
    pub name: &'static str,
    pub usb: Usb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
pub struct Usb {
    pub vendor_id: u16,
    pub product_id: u16,
    /// The HID interface the driver talks to.
    pub interface: i32,
}

/// Something on the device the configuration can put a key on.
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct Control {
    /// Where its key is kept in a page.
    pub index: u8,
    pub kind: ControlKind,
    pub cell: Cell,
    /// Where it sits on the panel.
    pub bounds: Rect,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ControlKind {
    /// A key with a screen, drawn as a whole image of this size.
    Key { image: Size },
}

/// The control's place in the grid of the layout, with the cells it spans.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
pub struct Cell {
    pub row: u8,
    pub column: u8,
    pub row_span: u8,
    pub column_span: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

/// How the plugin SDKs are told about the device.
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
pub struct PluginIdentity {
    /// Elgato's device type for the Stream Deck with the same keypad, or 0.
    pub elgato_type: u8,
    /// The model name Ulanzi's plugins filter their actions by.
    pub ulanzi_model: Option<&'static str>,
}

impl DeviceModel {
    pub fn control(&self, index: u8) -> Option<&Control> {
        self.controls.iter().find(|c| c.index == index)
    }

    /// The size a key on `index` is drawn at, if the control has a screen.
    pub fn image_size(&self, index: u8) -> Option<Size> {
        self.control(index).map(|control| match control.kind {
            ControlKind::Key { image } => image,
        })
    }

    pub fn indexes(&self) -> impl Iterator<Item = u8> + '_ {
        self.controls.iter().map(|c| c.index)
    }

    /// Where a key of this model is in the grid, as plugins name it: `column_row`.
    pub fn key_name(&self, index: u8) -> String {
        let cell = self.control(index).map(|c| c.cell).unwrap_or(Cell {
            row: 0,
            column: 0,
            row_span: 1,
            column_span: 1,
        });
        format!("{}_{}", cell.column, cell.row)
    }

    pub fn matches(&self, vendor_id: u16, product_id: u16, interface: i32) -> bool {
        self.variants.iter().any(|v| {
            v.usb.vendor_id == vendor_id
                && v.usb.product_id == product_id
                && v.usb.interface == interface
        })
    }
}

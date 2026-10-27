pub const VENDOR_ID: u16 = 0x2207;
pub const PRODUCT_ID: u16 = 0x0019;
pub const DATA_INTERFACE: i32 = 0;

pub const COLUMNS: u8 = 5;
pub const KEY_COUNT: u8 = 13;
/// The wide key spans two cells at column 3, row 2 and reports index 13.
pub const WIDE_KEY: u8 = 13;

pub const KEY_SIZE: (u32, u32) = (196, 196);
/// The panel is 1220x708: 196 px keys with 60 px gaps. The wide key covers two keys and the
/// gap between them; the firmware stretches whatever it gets to this size.
pub const WIDE_KEY_SIZE: (u32, u32) = (452, 196);

use std::io::Cursor;

use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC,
    DeleteObject, GetDIBits, GetObjectW, HBITMAP, HGDIOBJ,
};
use windows::Win32::UI::Shell::SHDefExtractIconW;
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};
use windows::core::HSTRING;

use super::apps::program_of;

/// Pixels; the UI shows icons at a third of this, so they stay sharp on dense screens.
const SIZE: u32 = 64;

/// The icon of the app with this id, as a PNG.
pub fn app_icon(id: &str) -> Option<Vec<u8>> {
    let program = program_of(id)?;
    let mut icon = HICON::default();
    // SAFETY: the path outlives the call, and the icon it returns is destroyed below.
    unsafe {
        SHDefExtractIconW(
            &HSTRING::from(program.as_os_str()),
            0,
            0,
            Some(&mut icon),
            None,
            SIZE,
        )
        .ok()
        .ok()?;
    }
    if icon.is_invalid() {
        return None;
    }
    let png = to_png(icon);
    // SAFETY: an icon this function got from the system and nothing else holds.
    unsafe {
        let _ = DestroyIcon(icon);
    }
    png
}

fn to_png(icon: HICON) -> Option<Vec<u8>> {
    let mut info = ICONINFO::default();
    // SAFETY: `info` outlives the call; its bitmaps are deleted below.
    unsafe { GetIconInfo(icon, &mut info).ok()? };
    let color = pixels(info.hbmColor);
    let mask = pixels(info.hbmMask);
    // SAFETY: bitmaps GetIconInfo created for this call only.
    unsafe {
        let _ = DeleteObject(HGDIOBJ(info.hbmColor.0));
        let _ = DeleteObject(HGDIOBJ(info.hbmMask.0));
    }
    let (width, height, mut bgra) = color?;
    // Old icons carry no transparency of their own, only a mask: white where nothing is drawn.
    if bgra.chunks(4).all(|pixel| pixel[3] == 0) {
        let (_, _, mask) = mask?;
        for (pixel, mask) in bgra.chunks_mut(4).zip(mask.chunks(4)) {
            pixel[3] = if mask[0] == 0 { 255 } else { 0 };
        }
    }
    for pixel in bgra.chunks_mut(4) {
        pixel.swap(0, 2);
    }
    let mut png = Vec::new();
    let mut encoder = png::Encoder::new(Cursor::new(&mut png), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().ok()?;
    writer.write_image_data(&bgra).ok()?;
    writer.finish().ok()?;
    Some(png)
}

/// The pixels of a bitmap, top row first, four bytes each in blue, green, red, alpha.
fn pixels(bitmap: HBITMAP) -> Option<(u32, u32, Vec<u8>)> {
    if bitmap.is_invalid() {
        return None;
    }
    let mut size = BITMAP::default();
    // SAFETY: `size` outlives the call and is the size given.
    let read = unsafe {
        GetObjectW(
            HGDIOBJ(bitmap.0),
            size_of::<BITMAP>() as i32,
            Some(&mut size as *mut BITMAP as *mut _),
        )
    };
    if read == 0 || size.bmWidth <= 0 || size.bmHeight <= 0 {
        return None;
    }
    let (width, height) = (size.bmWidth as u32, size.bmHeight as u32);
    let mut info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            biHeight: -(height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut bytes = vec![0u8; (width * height * 4) as usize];
    // SAFETY: the buffer holds the whole bitmap at 32 bits a pixel, and the context is deleted.
    unsafe {
        let context = CreateCompatibleDC(None);
        let lines = GetDIBits(
            context,
            bitmap,
            0,
            height,
            Some(bytes.as_mut_ptr().cast()),
            &mut info,
            DIB_RGB_COLORS,
        );
        let _ = DeleteDC(context);
        (lines == height as i32).then_some((width, height, bytes))
    }
}

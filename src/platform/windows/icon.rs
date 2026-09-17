//! Extract an executable's icon as RGBA pixels, Task Manager style.
//!
//! Uses the shell's icon lookup (`SHGetFileInfoW`), so the result matches
//! what Explorer / Task Manager show for the same executable. Handles
//! 32-bit alpha icons and falls back to the AND mask for legacy 24-bit icons.

use std::path::Path;

use windows::core::HSTRING;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO,
    BITMAPINFOHEADER, DIB_RGB_COLORS, HBITMAP, HDC,
};
use windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES;
use windows::Win32::UI::Shell::{SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON};
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};

/// Extract the large (typically 32×32) icon of `path` as `(width, height, straight RGBA)`.
pub fn extract_icon_rgba(path: &Path) -> Option<(u32, u32, Vec<u8>)> {
    let (icon, rgba) = unsafe {
        let mut fi = SHFILEINFOW::default();
        let ok = SHGetFileInfoW(
            &HSTRING::from(path),
            FILE_FLAGS_AND_ATTRIBUTES(0),
            Some(&mut fi),
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_LARGEICON,
        );
        if ok == 0 {
            return None;
        }
        (fi.hIcon, icon_to_rgba(fi.hIcon))
    };
    unsafe {
        let _ = DestroyIcon(icon);
    }
    rgba
}

fn icon_to_rgba(icon: HICON) -> Option<(u32, u32, Vec<u8>)> {
    let (color, mask) = unsafe {
        let mut info = ICONINFO::default();
        GetIconInfo(icon, &mut info).ok()?;
        (info.hbmColor, info.hbmMask)
    };
    let result = bitmaps_to_rgba(color, mask);
    unsafe {
        let _ = DeleteObject(color.into());
        let _ = DeleteObject(mask.into());
    }
    result
}

fn bitmaps_to_rgba(color: HBITMAP, mask: HBITMAP) -> Option<(u32, u32, Vec<u8>)> {
    let (w, h) = unsafe {
        let mut bmp = BITMAP::default();
        if GetObjectW(
            color.into(),
            std::mem::size_of::<BITMAP>() as i32,
            Some(&mut bmp as *mut _ as *mut _),
        ) == 0
        {
            return None;
        }
        (bmp.bmWidth.unsigned_abs(), bmp.bmHeight.unsigned_abs())
    };
    if w == 0 || h == 0 {
        return None;
    }
    let hdc = unsafe { GetDC(None) };
    let result = dib_to_rgba(hdc, color, mask, w, h);
    unsafe {
        ReleaseDC(None, hdc);
    }
    result
}

fn dib_to_rgba(
    hdc: HDC,
    color: HBITMAP,
    mask: HBITMAP,
    w: u32,
    h: u32,
) -> Option<(u32, u32, Vec<u8>)> {
    let mut pixels = vec![0u8; (w * h * 4) as usize];
    let mut bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w as i32,
            biHeight: -(h as i32), // top-down rows
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let copied = unsafe {
        GetDIBits(
            hdc,
            color,
            0,
            h,
            Some(pixels.as_mut_ptr() as *mut _),
            &mut bmi,
            DIB_RGB_COLORS,
        )
    };
    if copied != h as i32 {
        return None;
    }

    // Legacy 24-bit icons carry no alpha channel; every alpha byte is 0.
    if !pixels.chunks_exact(4).any(|c| c[3] != 0) {
        if let Some(mask_bits) = and_mask_bits(hdc, mask, w, h) {
            let stride = ((w as usize + 31) / 32) * 4;
            for (i, c) in pixels.chunks_exact_mut(4).enumerate() {
                let x = i % w as usize;
                let y = i / w as usize;
                // AND mask: set bit = transparent.
                let opaque = mask_bits[y * stride + x / 8] & (0x80 >> (x % 8)) == 0;
                c[3] = if opaque { 255 } else { 0 };
            }
        } else {
            // No usable mask: keep the icon opaque rather than invisible.
            for c in pixels.chunks_exact_mut(4) {
                c[3] = 255;
            }
        }
    }

    // BGRA -> RGBA.
    for c in pixels.chunks_exact_mut(4) {
        c.swap(0, 2);
    }
    Some((w, h, pixels))
}

fn and_mask_bits(hdc: HDC, mask: HBITMAP, w: u32, h: u32) -> Option<Vec<u8>> {
    let stride = ((w as usize + 31) / 32) * 4;
    let mut bits = vec![0u8; stride * h as usize];
    let mut bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w as i32,
            biHeight: -(h as i32), // top-down rows
            biPlanes: 1,
            biBitCount: 1,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let copied = unsafe {
        GetDIBits(
            hdc,
            mask,
            0,
            h,
            Some(bits.as_mut_ptr() as *mut _),
            &mut bmi,
            DIB_RGB_COLORS,
        )
    };
    if copied != h as i32 {
        return None;
    }
    Some(bits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_icon_from_own_exe() {
        let exe = std::env::current_exe().expect("exe path");
        let (w, h, rgba) = extract_icon_rgba(&exe).expect("app exe embeds an icon");
        assert!(w >= 16 && h >= 16, "unexpected icon size {w}x{h}");
        assert_eq!(rgba.len(), (w * h * 4) as usize);
        assert!(
            rgba.chunks_exact(4).any(|c| c[3] > 0),
            "icon should have visible pixels"
        );
    }
}

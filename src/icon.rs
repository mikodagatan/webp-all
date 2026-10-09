//! Menu bar icon (a framed photo), drawn in code so no image assets need bundling.
//! On macOS it's a template image: only alpha matters, macOS tints it for light/dark menu bars.
//! Windows and Linux draw it as-is on a panel that may be light or dark, so it's mid-blue there.

use tray_icon::Icon;

/// `inside` draws on a SIZE×SIZE grid; the tray icon uses it 1:1.
const SIZE: u32 = 36;
const SUPERSAMPLE: u32 = 4;
const COLOR: [u8; 3] = if cfg!(target_os = "macos") { [0, 0, 0] } else { [0x3b, 0x82, 0xf6] };

pub fn tray_icon() -> Icon {
    Icon::from_rgba(rgba(SIZE), SIZE, SIZE).expect("icon dimensions match buffer")
}

/// The icon as `size`×`size` RGBA pixels.
pub fn rgba(size: u32) -> Vec<u8> {
    let scale = SIZE as f32 / size as f32;
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);
    for py in 0..size {
        for px in 0..size {
            let mut hits = 0;
            for sy in 0..SUPERSAMPLE {
                for sx in 0..SUPERSAMPLE {
                    let x = (px as f32 + (sx as f32 + 0.5) / SUPERSAMPLE as f32) * scale;
                    let y = (py as f32 + (sy as f32 + 0.5) / SUPERSAMPLE as f32) * scale;
                    hits += u32::from(inside(x, y));
                }
            }
            let alpha = (hits * 255 / (SUPERSAMPLE * SUPERSAMPLE)) as u8;
            rgba.extend_from_slice(&[COLOR[0], COLOR[1], COLOR[2], alpha]);
        }
    }
    rgba
}

fn inside(x: f32, y: f32) -> bool {
    let frame = rounded_rect(x, y, (2.0, 6.0, 34.0, 30.0), 4.0)
        && !rounded_rect(x, y, (5.0, 9.0, 31.0, 27.0), 1.5);
    let in_picture = rounded_rect(x, y, (5.0, 9.0, 31.0, 27.0), 1.5);
    let big_mountain = y >= 14.0 + (x - 13.0).abs();
    let small_mountain = y >= 18.0 + (x - 24.0).abs();
    let sun = (x - 25.0).powi(2) + (y - 13.5).powi(2) <= 2.5f32.powi(2);
    frame || (in_picture && (big_mountain || small_mountain || sun))
}

fn rounded_rect(x: f32, y: f32, (x0, y0, x1, y1): (f32, f32, f32, f32), r: f32) -> bool {
    let dx = (x0 + r - x).max(x - (x1 - r)).max(0.0);
    let dy = (y0 + r - y).max(y - (y1 - r)).max(0.0);
    x >= x0 && x <= x1 && y >= y0 && y <= y1 && dx * dx + dy * dy <= r * r
}

//! The cover in true color with half blocks: each cell is two square
//! pixels, the upper one as the glyph and the lower one as the background.
//! No palette, no dithering — the cover as it is, only coarse.

use image::RgbaImage;

use super::{Cell, Picture, Size, sample_grid, u8c};

pub fn render_smooth(img: &RgbaImage, size: Size) -> Picture {
    if size.width == 0 || size.height == 0 || img.width() == 0 || img.height() == 0 {
        return Vec::new();
    }
    let (gw, gh) = (size.width, size.height * 2);
    let px = sample_grid(img, gw, gh);
    (0..size.height)
        .map(|row| {
            (0..size.width)
                .map(|col| Cell {
                    ch: '▀',
                    fg: u8c(px[2 * row * gw + col]),
                    bg: u8c(px[(2 * row + 1) * gw + col]),
                })
                .collect()
        })
        .collect()
}

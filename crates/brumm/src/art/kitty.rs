//! The kitty graphics protocol shows the cover at the terminal's full
//! resolution. It is placed with Unicode placeholders: the image is sent
//! once, and the cells that show it are ordinary text — a private-use
//! character whose foreground color names the image and whose diacritics
//! name the row and column — so the screen's renderer moves, clips and
//! redraws it like any other text. kitty and Ghostty support it.

use std::io::Cursor;

use base64::Engine as _;
use image::RgbaImage;

use super::Size;

/// The placeholder character.
pub const PLACEHOLDER: char = '\u{10EEEE}';

/// The row and column diacritics, as ranges, from kitty's
/// rowcolumn-diacritics.txt.
const DIACRITICS: &[(u32, u32)] = &[
    (0x0305, 0x0305),
    (0x030D, 0x030E),
    (0x0310, 0x0310),
    (0x0312, 0x0312),
    (0x033D, 0x033F),
    (0x0346, 0x0346),
    (0x034A, 0x034C),
    (0x0350, 0x0352),
    (0x0357, 0x0357),
    (0x035B, 0x035B),
    (0x0363, 0x036F),
    (0x0483, 0x0487),
    (0x0592, 0x0595),
    (0x0597, 0x0599),
    (0x059C, 0x05A1),
    (0x05A8, 0x05A9),
    (0x05AB, 0x05AC),
    (0x05AF, 0x05AF),
    (0x05C4, 0x05C4),
    (0x0610, 0x0617),
    (0x0657, 0x065B),
    (0x065D, 0x065E),
    (0x06D6, 0x06DC),
    (0x06DF, 0x06E2),
    (0x06E4, 0x06E4),
    (0x06E7, 0x06E8),
    (0x06EB, 0x06EC),
    (0x0730, 0x0730),
    (0x0732, 0x0733),
    (0x0735, 0x0736),
    (0x073A, 0x073A),
    (0x073D, 0x073D),
    (0x073F, 0x0741),
    (0x0743, 0x0743),
    (0x0745, 0x0745),
    (0x0747, 0x0747),
    (0x0749, 0x074A),
    (0x07EB, 0x07F1),
    (0x07F3, 0x07F3),
    (0x0816, 0x0819),
    (0x081B, 0x0823),
    (0x0825, 0x0827),
    (0x0829, 0x082D),
    (0x0951, 0x0951),
    (0x0953, 0x0954),
    (0x0F82, 0x0F83),
    (0x0F86, 0x0F87),
    (0x135D, 0x135F),
    (0x17DD, 0x17DD),
    (0x193A, 0x193A),
    (0x1A17, 0x1A17),
    (0x1A75, 0x1A7C),
    (0x1B6B, 0x1B6B),
    (0x1B6D, 0x1B73),
    (0x1CD0, 0x1CD2),
    (0x1CDA, 0x1CDB),
    (0x1CE0, 0x1CE0),
    (0x1DC0, 0x1DC1),
    (0x1DC3, 0x1DC9),
    (0x1DCB, 0x1DCC),
    (0x1DD1, 0x1DE6),
    (0x1DFE, 0x1DFE),
    (0x20D0, 0x20D1),
    (0x20D4, 0x20D7),
    (0x20DB, 0x20DC),
    (0x20E1, 0x20E1),
    (0x20E7, 0x20E7),
    (0x20E9, 0x20E9),
    (0x20F0, 0x20F0),
    (0x2CEF, 0x2CF1),
    (0x2DE0, 0x2DFF),
    (0xA66F, 0xA66F),
    (0xA67C, 0xA67D),
    (0xA6F0, 0xA6F1),
    (0xA8E0, 0xA8F1),
    (0xAAB0, 0xAAB0),
    (0xAAB2, 0xAAB3),
    (0xAAB7, 0xAAB8),
    (0xAABE, 0xAABF),
    (0xAAC1, 0xAAC1),
    (0xFE20, 0xFE26),
    (0x10A0F, 0x10A0F),
    (0x10A38, 0x10A38),
    (0x1D185, 0x1D189),
    (0x1D1AA, 0x1D1AD),
    (0x1D242, 0x1D244),
];

/// The diacritic for row or column i; out of range, the first.
pub fn diacritic(mut i: usize) -> char {
    for &(a, b) in DIACRITICS {
        let n = (b - a + 1) as usize;
        if i < n {
            return char::from_u32(a + i as u32).unwrap_or('\u{0305}');
        }
        i -= n;
    }
    '\u{0305}'
}

/// Guesses from the environment whether the terminal speaks the kitty
/// graphics protocol with Unicode placeholders.
pub fn kitty_supported() -> bool {
    let var = |k: &str| std::env::var(k).unwrap_or_default();
    !var("KITTY_WINDOW_ID").is_empty()
        || var("TERM") == "xterm-kitty"
        || var("TERM_PROGRAM") == "ghostty"
        || !var("GHOSTTY_RESOURCES_DIR").is_empty()
}

/// The escape sequence that uploads img as image id, fitted into a box of
/// size cells. Ids are 16–255, so the 256-color foreground that names
/// them survives any color handling.
pub fn kitty_send(id: u8, img: &RgbaImage, size: Size) -> Option<String> {
    let mut png = Vec::new();
    image::DynamicImage::ImageRgba8(img.clone())
        .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
        .ok()?;
    let data = base64::engine::general_purpose::STANDARD.encode(png);
    const CHUNK: usize = 4096;
    let mut out = String::with_capacity(data.len() + data.len() / CHUNK * 16 + 64);
    let mut i = 0;
    loop {
        let part = &data[i..(i + CHUNK).min(data.len())];
        let more = u8::from(i + CHUNK < data.len());
        if i == 0 {
            // a=T with U=1: transmit and make a virtual placement for the
            // placeholders; q=2: no replies to swallow.
            out.push_str(&format!(
                "\x1b_Ga=T,U=1,f=100,i={id},c={},r={},q=2,m={more};{part}\x1b\\",
                size.width, size.height
            ));
        } else {
            out.push_str(&format!("\x1b_Gm={more};{part}\x1b\\"));
        }
        i += CHUNK;
        if i >= data.len() {
            return Some(out);
        }
    }
}

/// Frees image id in the terminal.
pub fn kitty_delete(id: u8) -> String {
    format!("\x1b_Ga=d,d=I,i={id},q=2\x1b\\")
}

/// The placeholder cells that show image id in a box of size: a cell's
/// text per row and column, its foreground the 256-color id.
pub fn kitty_lines(size: Size) -> Vec<Vec<String>> {
    (0..size.height)
        .map(|r| {
            let row = diacritic(r);
            (0..size.width)
                .map(|c| [PLACEHOLDER, row, diacritic(c)].iter().collect())
                .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use unicode_width::UnicodeWidthStr;

    #[test]
    fn the_diacritic_table_is_kittys() {
        let n: u32 = DIACRITICS.iter().map(|(a, b)| b - a + 1).sum();
        assert_eq!(n, 297);
        assert_eq!(diacritic(0), '\u{0305}');
        assert_eq!(diacritic(1), '\u{030D}');
        assert_eq!(diacritic(296), '\u{1D244}');
        assert_eq!(diacritic(9999), '\u{0305}');
    }

    /// A placeholder cell is one column wide: the layout counts on it.
    #[test]
    fn placeholder_cells_are_one_column() {
        let lines = kitty_lines(Size {
            width: 3,
            height: 2,
        });
        assert_eq!(lines.len(), 2);
        for cell in lines.iter().flatten() {
            assert_eq!(cell.width(), 1, "{cell:?}");
        }
    }

    #[test]
    fn upload_chunks() {
        let img = RgbaImage::from_fn(64, 64, |x, y| {
            image::Rgba([(x * 3) as u8, (y * 3) as u8, (x ^ y) as u8, 255])
        });
        let s = kitty_send(
            42,
            &img,
            Size {
                width: 10,
                height: 5,
            },
        )
        .unwrap();
        assert!(s.starts_with("\x1b_Ga=T,U=1,f=100,i=42,c=10,r=5,q=2,m="));
        assert!(s.ends_with("\x1b\\"));
        assert!(s.contains("m=0;"), "the last chunk says so");
        assert_eq!(kitty_delete(42), "\x1b_Ga=d,d=I,i=42,q=2\x1b\\");
    }
}

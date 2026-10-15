//! Album covers: fetched through a disk cache shared with the window, and
//! drawn for the terminal as dithered sextant pixel art, smooth half
//! blocks, or the real image through the kitty graphics protocol.

mod dither;
mod kitty;
mod smooth;

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use image::RgbaImage;
use sha2::{Digest, Sha256};

pub use dither::render_dithered;
pub use kitty::{kitty_delete, kitty_lines, kitty_send, kitty_supported};
pub use smooth::render_smooth;

use crate::error::{Result, err};

/// A render target in terminal cells.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Size {
    pub width: usize,
    pub height: usize,
}

/// One drawn cell: a glyph in a foreground over a background, both true
/// color.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cell {
    pub ch: char,
    pub fg: [u8; 3],
    pub bg: [u8; 3],
}

/// A drawn cover: rows of cells.
pub type Picture = Vec<Vec<Cell>>;

const MAX_BYTES: usize = 5 << 20;
const MAX_PIXELS: u64 = 16_000_000;

/// A kept cover's longer side. Apple serves 600 px; the largest cover
/// brumm draws needs ~2 pixels per cell across, so 320 keeps full detail
/// at a fraction of the memory.
const KEEP_PIXELS: u32 = 320;

/// The cover cache's file for url, as the window names it too.
pub fn cache_path(dir: &Path, url: &str) -> PathBuf {
    let sum = Sha256::digest(url.as_bytes());
    dir.join(
        sum[..12]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>(),
    )
}

/// A cover from the disk cache, downloaded on a miss. Covers never change
/// at an address; prune bounds the cache by dropping the least recently
/// used.
pub async fn cached(client: &reqwest::Client, dir: &Path, url: &str) -> Result<RgbaImage> {
    let path = cache_path(dir, url);
    if std::fs::metadata(&path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.elapsed().ok())
        .is_some_and(|age| age > std::time::Duration::from_secs(86400))
    {
        let _ = tokio::fs::remove_file(&path).await;
    }
    if let Ok(b) = tokio::fs::read(&path).await
        && let Ok(img) = decode(&b, true)
    {
        // Recently used: pruned last.
        if let Ok(f) = std::fs::File::options().write(true).open(&path) {
            let _ = f.set_modified(SystemTime::now());
        }
        return Ok(img);
    }
    let b = download(client, url).await?;
    let img = decode(&b, true)?;
    if std::fs::create_dir_all(dir).is_ok() {
        let tmp = dir.join(format!(".cover-{}", crate::engine::random_hex(6)));
        if std::fs::write(&tmp, &b).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        } else {
            let _ = std::fs::remove_file(&tmp);
        }
    }
    Ok(img)
}

/// Deletes the least recently used covers in dir until the rest fit in
/// max bytes.
pub fn prune(dir: &Path, max: u64) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(PathBuf, u64, SystemTime)> = entries
        .flatten()
        .filter_map(|e| {
            let m = e.metadata().ok()?;
            m.is_file().then(|| {
                (
                    e.path(),
                    m.len(),
                    m.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                )
            })
        })
        .collect();
    let mut total: u64 = files.iter().map(|f| f.1).sum();
    files.sort_by_key(|f| f.2);
    for (p, size, _) in files {
        if total <= max {
            break;
        }
        if std::fs::remove_file(p).is_ok() {
            total -= size;
        }
    }
}

async fn download(client: &reqwest::Client, url: &str) -> Result<Vec<u8>> {
    let mut resp = client
        .get(url)
        .timeout(Duration::from_secs(15))
        .send()
        .await?;
    if resp.status() != reqwest::StatusCode::OK {
        return Err(err(format!("cover: {}", resp.status())));
    }
    let mut out = Vec::new();
    while let Some(chunk) = resp.chunk().await? {
        out.extend_from_slice(&chunk);
        if out.len() > MAX_BYTES {
            return Err(err(format!("cover: larger than {MAX_BYTES} bytes")));
        }
    }
    Ok(out)
}

/// Decodes an image, refusing anything oversized before the decode
/// allocates it; with keep_small, shrunk to KEEP_PIXELS.
fn decode(b: &[u8], keep_small: bool) -> Result<RgbaImage> {
    let reader = image::ImageReader::new(Cursor::new(b)).with_guessed_format()?;
    let (w, h) = reader.into_dimensions().map_err(|e| err(e.to_string()))?;
    if w == 0 || h == 0 || w as u64 * h as u64 > MAX_PIXELS {
        return Err(err(format!("cover: bad dimensions {w}x{h}")));
    }
    let img = image::ImageReader::new(Cursor::new(b))
        .with_guessed_format()?
        .decode()
        .map_err(|e| err(e.to_string()))?
        .to_rgba8();
    Ok(if keep_small {
        shrink(img, KEEP_PIXELS)
    } else {
        img
    })
}

/// A cover at its full size, from the disk cache cached fills.
pub fn original(dir: &Path, url: &str) -> Result<RgbaImage> {
    decode(&std::fs::read(cache_path(dir, url))?, false)
}

/// Box-filters img so its longer side is at most n.
fn shrink(img: RgbaImage, n: u32) -> RgbaImage {
    let (sw, sh) = img.dimensions();
    if sw <= n && sh <= n {
        return img;
    }
    let (dw, dh) = if sh > sw {
        (sw * n / sh, n)
    } else {
        (n, sh * n / sw)
    };
    let (dw, dh) = (dw.max(1), dh.max(1));
    let src = img.as_raw();
    let mut dst = RgbaImage::new(dw, dh);
    for y in 0..dh {
        let (y0, y1) = (y * sh / dh, ((y + 1) * sh / dh).max(y * sh / dh + 1));
        for x in 0..dw {
            let (x0, x1) = (x * sw / dw, ((x + 1) * sw / dw).max(x * sw / dw + 1));
            let mut acc = [0u32; 4];
            let mut cnt = 0u32;
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let o = ((sy * sw + sx) * 4) as usize;
                    for k in 0..4 {
                        acc[k] += src[o + k] as u32;
                    }
                    cnt += 1;
                }
            }
            dst.put_pixel(
                x,
                y,
                image::Rgba([
                    (acc[0] / cnt) as u8,
                    (acc[1] / cnt) as u8,
                    (acc[2] / cnt) as u8,
                    (acc[3] / cnt) as u8,
                ]),
            );
        }
    }
    dst
}

/// img darker, for behind a card.
pub fn dimmed(img: &RgbaImage) -> RgbaImage {
    let mut out = img.clone();
    for p in out.pixels_mut() {
        p.0 = [
            (p.0[0] as u32 * 2 / 5) as u8,
            (p.0[1] as u32 * 2 / 5) as u8,
            (p.0[2] as u32 * 2 / 5) as u8,
            255,
        ];
    }
    out
}

/// Area-averages img into a gw × gh grid (a box filter) so a heavily
/// downscaled cover stays recognisable.
pub(crate) fn sample_grid(img: &RgbaImage, gw: usize, gh: usize) -> Vec<[f64; 3]> {
    let (src_w, src_h) = (img.width() as usize, img.height() as usize);
    let px = img.as_raw();
    let mut out = vec![[0f64; 3]; gw * gh];
    for py in 0..gh {
        let y0 = py * src_h / gh;
        let y1 = ((py + 1) * src_h / gh).max(y0 + 1);
        for pxi in 0..gw {
            let x0 = pxi * src_w / gw;
            let x1 = ((pxi + 1) * src_w / gw).max(x0 + 1);
            let (mut r, mut g, mut b, mut n) = (0f64, 0f64, 0f64, 0f64);
            for y in y0..y1.min(src_h) {
                let row = y * src_w * 4;
                for x in x0..x1.min(src_w) {
                    let o = row + x * 4;
                    r += px[o] as f64;
                    g += px[o + 1] as f64;
                    b += px[o + 2] as f64;
                    n += 1.0;
                }
            }
            if n > 0.0 {
                out[py * gw + pxi] = [r / n, g / n, b / n];
            }
        }
    }
    out
}

pub(crate) fn u8c(c: [f64; 3]) -> [u8; 3] {
    let u = |v: f64| v.round().clamp(0.0, 255.0) as u8;
    [u(c[0]), u(c[1]), u(c[2])]
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn test_image(w: u32, h: u32) -> RgbaImage {
        RgbaImage::from_fn(w, h, |x, y| {
            image::Rgba([
                (x * 255 / w.max(1)) as u8,
                (y * 255 / h.max(1)) as u8,
                120,
                255,
            ])
        })
    }

    #[test]
    fn shrink_and_dimensions() {
        let big = test_image(640, 480);
        let small = shrink(big, KEEP_PIXELS);
        assert_eq!(small.dimensions(), (320, 240));
        let mut png = Vec::new();
        image::DynamicImage::ImageRgba8(test_image(10, 10))
            .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        assert_eq!(decode(&png, true).unwrap().dimensions(), (10, 10));
        assert!(decode(b"not an image", true).is_err());
    }

    #[test]
    fn prune_keeps_the_recently_used() {
        let dir =
            std::env::temp_dir().join(format!("brumm-prune-{}", crate::engine::random_hex(6)));
        std::fs::create_dir_all(&dir).unwrap();
        for (i, name) in ["old", "mid", "new"].iter().enumerate() {
            let p = dir.join(name);
            std::fs::write(&p, vec![0u8; 100]).unwrap();
            let f = std::fs::File::options().write(true).open(&p).unwrap();
            f.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(1000 + i as u64 * 100))
                .unwrap();
        }
        prune(&dir, 250);
        assert!(!dir.join("old").exists() && dir.join("mid").exists() && dir.join("new").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cache_names_match_the_window() {
        // gui/src/covers.cpp: the first 12 bytes of the SHA-256, in hex.
        let p = cache_path(Path::new("/c"), "https://x/600x600.jpg");
        assert_eq!(p.file_name().unwrap().len(), 24);
    }
}

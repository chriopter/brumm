//! The C interface brumm-gui links (gui/src/brumm_shared.h). It is narrow
//! on purpose: a listener ticked once a frame, read back as plain numbers,
//! and the accent pick. Nothing here allocates on the caller's behalf but
//! the listener, freed with `brumm_listener_free`.

use crate::accent;
use crate::listen::Listener;

/// The listener's numbers after a tick, for the shaders' uniforms.
#[repr(C)]
#[derive(Default)]
pub struct BrummLevels {
    pub t: f64,
    pub live: f64,
    pub bass: f64,
    pub mid: f64,
    pub high: f64,
    pub level: f64,
    pub beat: f64,
    pub since: f64,
    pub kicks: i32,
}

/// A new listener; never null.
#[unsafe(no_mangle)]
pub extern "C" fn brumm_listener_new() -> *mut Listener {
    Box::into_raw(Box::default())
}

/// Frees a listener made by `brumm_listener_new`; null is ignored.
///
/// # Safety
/// `l` is null or came from `brumm_listener_new` and is not used again.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn brumm_listener_free(l: *mut Listener) {
    if !l.is_null() {
        // SAFETY: the caller hands back the box made by brumm_listener_new.
        drop(unsafe { Box::from_raw(l) });
    }
}

/// Moves the listener on by `dt` seconds (clamped to 0–0.1; NaN is 0) with `n`
/// bands of input, 0–1 each.
///
/// # Safety
/// `l` came from `brumm_listener_new`; `input` points at `n` doubles (or
/// is null with `n` 0).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn brumm_listener_tick(
    l: *mut Listener,
    input: *const f64,
    n: usize,
    dt: f64,
    playing: bool,
) {
    // SAFETY: as documented above, both pointers come valid from the caller.
    let (l, input) = unsafe {
        let Some(l) = l.as_mut() else { return };
        (
            l,
            if input.is_null() {
                &[][..]
            } else {
                std::slice::from_raw_parts(input, n)
            },
        )
    };
    l.listen(input, dt, playing); // listen bounds the step
}

/// The listener's levels after the last tick.
///
/// # Safety
/// `l` came from `brumm_listener_new`; `out` points at a BrummLevels.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn brumm_listener_levels(l: *const Listener, out: *mut BrummLevels) {
    // SAFETY: valid pointers from the caller, as documented.
    let (Some(l), Some(out)) = (unsafe { l.as_ref() }, unsafe { out.as_mut() }) else {
        return;
    };
    *out = BrummLevels {
        t: l.t,
        live: l.live,
        bass: l.bass,
        mid: l.mid,
        high: l.high,
        level: l.level,
        beat: l.beat,
        since: l.since,
        kicks: l.kicks,
    };
}

/// Which of the listener's bands `brumm_listener_bands` reads.
#[repr(C)]
pub enum BrummBands {
    Spectrum = 0,
    Smoothed = 1,
    Peaks = 2,
}

/// The listener's bands of one kind, as many as the last tick's input;
/// the pointer stays valid until the next tick. `n` gets their count.
///
/// # Safety
/// `l` came from `brumm_listener_new`; `n` points at a size_t.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn brumm_listener_bands(
    l: *const Listener,
    which: BrummBands,
    n: *mut usize,
) -> *const f64 {
    // SAFETY: valid pointers from the caller, as documented.
    let (Some(l), Some(n)) = (unsafe { l.as_ref() }, unsafe { n.as_mut() }) else {
        return std::ptr::null();
    };
    let v = match which {
        BrummBands::Spectrum => &l.spec,
        BrummBands::Smoothed => &l.sm,
        BrummBands::Peaks => &l.peak,
    };
    *n = v.len();
    v.as_ptr()
}

/// Picks a cover's accent: its dominant vivid color, readable on `bg`
/// (0xRRGGBB). Returns false for a gray cover; else `out` gets 0xRRGGBB.
///
/// # Safety
/// `rgba` points at `height` rows of `stride` bytes, RGBA; `out` at a u32.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn brumm_accent(
    rgba: *const u8,
    width: usize,
    height: usize,
    stride: usize,
    bg: u32,
    out: *mut u32,
) -> bool {
    if rgba.is_null() || out.is_null() || stride < width * 4 {
        return false;
    }
    // SAFETY: the caller guarantees height rows of stride bytes.
    let px = unsafe { std::slice::from_raw_parts(rgba, height * stride) };
    let Some(c) = accent::vivid(px, width, height, stride) else {
        return false;
    };
    let c = accent::readable(c, Some([(bg >> 16) as u8, (bg >> 8) as u8, bg as u8]));
    // SAFETY: out is valid, as checked above.
    unsafe { *out = (c[0] as u32) << 16 | (c[1] as u32) << 8 | c[2] as u32 };
    true
}

//! Test pattern generators (UYVY output).

#![deny(missing_docs)]

use omt_media::{rgb_to_uyvy_pixel, uyvy_from_rgb_frame};

/// Errors while loading user images.
#[derive(Debug, thiserror::Error)]
pub enum PatternError {
    /// Image crate failure.
    #[error(transparent)]
    Image(#[from] image::ImageError),
    /// Unsupported geometry.
    #[error("invalid image geometry")]
    InvalidGeometry,
}

/// Available built-in patterns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum PatternKind {
    /// Classic SMPTE-style 75% color bars (full-frame).
    #[default]
    SmpteColorBars,
    /// SMPTE RP 219 / HD multi-format color bars.
    SmpteHdColorBars,
    /// Black field.
    Black,
    /// White field.
    White,
    /// 50% gray field.
    Gray,
    /// Grid / crosshatch.
    Grid,
    /// Horizontal luma ramp.
    Ramp,
    /// User-supplied still image (RGB converted each call unless cached upstream).
    Image,
}

impl PatternKind {
    /// All built-in kinds except [`PatternKind::Image`].
    pub const fn builtins() -> &'static [PatternKind] {
        &[
            Self::SmpteColorBars,
            Self::SmpteHdColorBars,
            Self::Black,
            Self::White,
            Self::Gray,
            Self::Grid,
            Self::Ramp,
        ]
    }

    /// Stable UI / CLI id.
    pub const fn id(self) -> &'static str {
        match self {
            Self::SmpteColorBars => "smpte-bars",
            Self::SmpteHdColorBars => "smpte-hd-bars",
            Self::Black => "black",
            Self::White => "white",
            Self::Gray => "gray",
            Self::Grid => "grid",
            Self::Ramp => "ramp",
            Self::Image => "image",
        }
    }

    /// English label.
    pub const fn label_en(self) -> &'static str {
        match self {
            Self::SmpteColorBars => "SMPTE Color Bars",
            Self::SmpteHdColorBars => "SMPTE HD Color Bars",
            Self::Black => "Black",
            Self::White => "White",
            Self::Gray => "Gray 50%",
            Self::Grid => "Grid",
            Self::Ramp => "Luma Ramp",
            Self::Image => "Image",
        }
    }

    /// Japanese label.
    pub const fn label_ja(self) -> &'static str {
        match self {
            Self::SmpteColorBars => "SMPTEカラーバー",
            Self::SmpteHdColorBars => "SMPTE HDカラーバー",
            Self::Black => "黒",
            Self::White => "白",
            Self::Gray => "グレー 50%",
            Self::Grid => "グリッド",
            Self::Ramp => "輝度ランプ",
            Self::Image => "画像",
        }
    }
}

/// 75% SMPTE-style bars (R,G,B).
const BARS_RGB: [(u8, u8, u8); 8] = [
    (191, 191, 191),
    (191, 191, 0),
    (0, 191, 191),
    (0, 191, 0),
    (191, 0, 191),
    (191, 0, 0),
    (0, 0, 191),
    (0, 0, 0),
];

// SMPTE RP 219 / FFmpeg smptehdbars YCbCr (studio) values: (Y, Cb, Cr).
const HD_RAINBOW: [(u8, u8, u8); 7] = [
    (180, 128, 128), // 75% white
    (168, 44, 136),  // 75% yellow
    (145, 147, 44),  // 75% cyan
    (133, 63, 52),   // 75% green
    (63, 193, 204),  // 75% magenta
    (51, 109, 212),  // 75% red
    (28, 212, 120),  // 75% blue
];
const HD_GRAY40: (u8, u8, u8) = (104, 128, 128);
const HD_GRAY15: (u8, u8, u8) = (49, 128, 128);
const HD_CYAN100: (u8, u8, u8) = (188, 154, 16);
const HD_YELLOW100: (u8, u8, u8) = (219, 16, 138);
const HD_BLUE100: (u8, u8, u8) = (32, 240, 118);
const HD_RED100: (u8, u8, u8) = (63, 102, 240);
const HD_WHITE100: (u8, u8, u8) = (235, 128, 128);
const HD_BLACK0: (u8, u8, u8) = (16, 128, 128);
const HD_BLACK2: (u8, u8, u8) = (20, 128, 128);
const HD_BLACK4: (u8, u8, u8) = (25, 128, 128);
const HD_NEG2: (u8, u8, u8) = (12, 128, 128);
const HD_I: (u8, u8, u8) = (57, 156, 97);
const HD_Q: (u8, u8, u8) = (44, 171, 147);

/// Fill a UYVY buffer with the requested pattern.
///
/// `phase_x` / `phase_y` in `0..1` enable independent horizontal / vertical
/// animation (scroll / moving marker) when meaningful for the pattern.
pub fn fill_uyvy(
    kind: PatternKind,
    dst: &mut [u8],
    width: i32,
    height: i32,
    phase_x: f32,
    phase_y: f32,
) {
    let w = width.max(2) as usize;
    let h = height.max(1) as usize;
    let stride = w * 2;
    assert!(dst.len() >= stride * h);

    match kind {
        PatternKind::SmpteColorBars => fill_bars(dst, w, h, stride, phase_x, phase_y),
        PatternKind::SmpteHdColorBars => fill_hd_bars(dst, w, h, stride, phase_x, phase_y),
        PatternKind::Black => fill_solid(dst, w, h, stride, 0, 0, 0),
        PatternKind::White => fill_solid(dst, w, h, stride, 235, 235, 235),
        PatternKind::Gray => fill_solid(dst, w, h, stride, 128, 128, 128),
        PatternKind::Grid => fill_grid(dst, w, h, stride, phase_x, phase_y),
        PatternKind::Ramp => fill_ramp(dst, w, h, stride, phase_x),
        PatternKind::Image => fill_solid(dst, w, h, stride, 16, 16, 16),
    }
}

/// Allocate and fill a UYVY frame.
pub fn generate_uyvy(
    kind: PatternKind,
    width: i32,
    height: i32,
    phase_x: f32,
    phase_y: f32,
) -> Vec<u8> {
    let mut buf = vec![0u8; (width.max(2) as usize) * 2 * (height.max(1) as usize)];
    fill_uyvy(kind, &mut buf, width, height, phase_x, phase_y);
    buf
}

/// Load an image file, resize to `width`×`height`, and convert to UYVY.
pub fn uyvy_from_image_path(
    path: &std::path::Path,
    width: i32,
    height: i32,
) -> Result<Vec<u8>, PatternError> {
    let img = image::open(path)?.to_rgb8();
    let resized = image::imageops::resize(
        &img,
        width.max(1) as u32,
        height.max(1) as u32,
        image::imageops::FilterType::Triangle,
    );
    Ok(uyvy_from_rgb_frame(
        resized.as_raw(),
        width.max(1) as u32,
        height.max(1) as u32,
    ))
}

fn phase_offset(phase: f32, extent: usize) -> usize {
    if phase == 0.0 || extent == 0 {
        0
    } else {
        ((phase.rem_euclid(1.0)) * extent as f32) as usize % extent
    }
}

fn fill_bars(dst: &mut [u8], w: usize, h: usize, stride: usize, phase_x: f32, phase_y: f32) {
    let offset = phase_offset(phase_x, w);
    let mut line = vec![0u8; stride];
    for x in (0..w).step_by(2) {
        let x0 = (x + offset) % w;
        let x1 = (x + 1 + offset) % w;
        let (y0, u0, v0) = rgb_to_uyvy_pixel_tuple(bar_at(x0, w));
        let (y1, u1, v1) = rgb_to_uyvy_pixel_tuple(bar_at(x1, w));
        write_uyvy_pair(&mut line, x * 2, (y0, u0, v0), (y1, u1, v1));
    }
    for y in 0..h {
        dst[y * stride..(y + 1) * stride].copy_from_slice(&line);
    }
    if phase_y != 0.0 {
        let my = phase_offset(phase_y, h);
        let row = &mut dst[my * stride..(my + 1) * stride];
        for x in (0..w).step_by(2) {
            let o = x * 2;
            row[o + 1] = 235;
            row[o + 3] = 235;
        }
    }
}

fn fill_hd_bars(dst: &mut [u8], w: usize, h: usize, stride: usize, phase_x: f32, phase_y: f32) {
    let ox = phase_offset(phase_x, w);
    let oy = phase_offset(phase_y, h);
    for y in 0..h {
        let sy = (y + oy) % h;
        for x in (0..w).step_by(2) {
            let x0 = (x + ox) % w;
            let x1 = (x + 1 + ox) % w;
            let (y0, u0, v0) = hd_yuv_at(x0, sy, w, h);
            let (y1, u1, v1) = hd_yuv_at(x1, sy, w, h);
            write_uyvy_pair(dst, y * stride + x * 2, (y0, u0, v0), (y1, u1, v1));
        }
    }
}

/// Sample RP 219 HD color-bar YCbCr at pixel (`x`, `y`).
fn hd_yuv_at(x: usize, y: usize, w: usize, h: usize) -> (u8, u8, u8) {
    let d_w = (w / 8).max(1);
    let r_w = ((w.div_ceil(4) * 3) / 7).max(1);
    let p1_h = (h * 7 / 12).max(1);
    let strip_h = (h / 12).max(1);
    let p2_y0 = p1_h;
    let p3_y0 = p1_h + strip_h;
    let p4_y0 = p1_h + 2 * strip_h;
    let l_w = d_w + 7 * r_w;

    if y < p2_y0 {
        if x < d_w {
            return HD_GRAY40;
        }
        let cx = x - d_w;
        if cx < 7 * r_w {
            return HD_RAINBOW[(cx / r_w).min(6)];
        }
        return HD_GRAY40;
    }

    if y < p3_y0 {
        if x < d_w {
            return HD_CYAN100;
        }
        if x < d_w + r_w {
            return HD_I;
        }
        if x < d_w + 7 * r_w {
            return HD_RAINBOW[0];
        }
        return HD_BLUE100;
    }

    if y < p4_y0 {
        if x < d_w {
            return HD_YELLOW100;
        }
        if x < d_w + r_w {
            return HD_Q;
        }
        let ramp_w = 6 * r_w;
        if x < d_w + r_w + ramp_w {
            let i = x - (d_w + r_w);
            let luma = ((i * 255) / ramp_w.max(1)) as u8;
            return (luma, 128, 128);
        }
        return HD_RED100;
    }

    if x < d_w {
        return HD_GRAY15;
    }

    let pluge = (r_w / 3).max(1);
    let spans: [(usize, (u8, u8, u8)); 8] = [
        ((r_w * 3 / 2).max(1), HD_BLACK0),
        ((r_w * 2).max(1), HD_WHITE100),
        ((r_w * 5 / 6).max(1), HD_BLACK0),
        (pluge, HD_NEG2),
        (pluge, HD_BLACK0),
        (pluge, HD_BLACK2),
        (pluge, HD_BLACK0),
        (pluge, HD_BLACK4),
    ];

    let mut cx = d_w;
    for (width, color) in spans {
        let end = cx + width;
        if x < end {
            return color;
        }
        cx = end;
    }
    if x < l_w.max(cx) {
        return HD_BLACK0;
    }
    HD_GRAY15
}

fn fill_solid(dst: &mut [u8], w: usize, h: usize, stride: usize, r: u8, g: u8, b: u8) {
    let (y, u, v) = rgb_to_uyvy_pixel(r, g, b);
    let mut line = vec![0u8; stride];
    for x in (0..w).step_by(2) {
        let o = x * 2;
        line[o] = u;
        line[o + 1] = y;
        line[o + 2] = v;
        line[o + 3] = y;
    }
    for row in 0..h {
        dst[row * stride..(row + 1) * stride].copy_from_slice(&line);
    }
}

fn fill_grid(dst: &mut [u8], w: usize, h: usize, stride: usize, phase_x: f32, phase_y: f32) {
    let spacing = 64usize;
    let shift_x = phase_offset(phase_x, spacing);
    let shift_y = phase_offset(phase_y, spacing);
    for y in 0..h {
        for x in (0..w).step_by(2) {
            let on = ((x + shift_x) % spacing == 0)
                || ((x + 1 + shift_x) % spacing == 0)
                || ((y + shift_y) % spacing == 0);
            let (r, g, b) = if on { (235, 235, 235) } else { (16, 16, 16) };
            let (y0, u0, v0) = rgb_to_uyvy_pixel(r, g, b);
            let (y1, u1, v1) = rgb_to_uyvy_pixel(r, g, b);
            write_uyvy_pair(dst, y * stride + x * 2, (y0, u0, v0), (y1, u1, v1));
        }
    }
}

fn fill_ramp(dst: &mut [u8], w: usize, h: usize, stride: usize, phase_x: f32) {
    let shift = phase_offset(phase_x, w);
    for y in 0..h {
        for x in (0..w).step_by(2) {
            let x0 = (x + shift) % w;
            let x1 = (x + 1 + shift) % w;
            let l0 = (16 + (x0 * 219) / w.max(1)) as u8;
            let l1 = (16 + (x1 * 219) / w.max(1)) as u8;
            let o = y * stride + x * 2;
            dst[o] = 128;
            dst[o + 1] = l0;
            dst[o + 2] = 128;
            dst[o + 3] = l1;
        }
    }
}

fn bar_at(x: usize, width: usize) -> (u8, u8, u8) {
    let idx = (x * BARS_RGB.len()) / width.max(1);
    BARS_RGB[idx.min(BARS_RGB.len() - 1)]
}

fn rgb_to_uyvy_pixel_tuple(rgb: (u8, u8, u8)) -> (u8, u8, u8) {
    rgb_to_uyvy_pixel(rgb.0, rgb.1, rgb.2)
}

fn write_uyvy_pair(dst: &mut [u8], o: usize, left: (u8, u8, u8), right: (u8, u8, u8)) {
    let (y0, u0, v0) = left;
    let (y1, u1, v1) = right;
    let u = ((u0 as u16 + u1 as u16) / 2) as u8;
    let v = ((v0 as u16 + v1 as u16) / 2) as u8;
    dst[o] = u;
    dst[o + 1] = y0;
    dst[o + 2] = v;
    dst[o + 3] = y1;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bars_have_expected_size() {
        let frame = generate_uyvy(PatternKind::SmpteColorBars, 64, 36, 0.0, 0.0);
        assert_eq!(frame.len(), 64 * 2 * 36);
        assert!(frame[1] > 150);
    }

    #[test]
    fn hd_bars_have_gray40_sides() {
        let w = 192;
        let h = 108;
        let frame = generate_uyvy(PatternKind::SmpteHdColorBars, w, h, 0.0, 0.0);
        assert_eq!(frame.len(), (w as usize) * 2 * (h as usize));
        assert!((100..=110).contains(&frame[1]));
    }

    #[test]
    fn black_is_studio_black() {
        let frame = generate_uyvy(PatternKind::Black, 16, 16, 0.0, 0.0);
        assert_eq!(frame[1], 16);
    }

    #[test]
    fn all_builtins_fill() {
        for kind in PatternKind::builtins() {
            let _ = generate_uyvy(*kind, 32, 18, 0.25, 0.5);
        }
    }

    #[test]
    fn independent_phases_change_grid() {
        let a = generate_uyvy(PatternKind::Grid, 128, 72, 0.0, 0.0);
        let b = generate_uyvy(PatternKind::Grid, 128, 72, 0.5, 0.0);
        let c = generate_uyvy(PatternKind::Grid, 128, 72, 0.0, 0.5);
        assert_ne!(a, b);
        assert_ne!(a, c);
        assert_ne!(b, c);
    }
}

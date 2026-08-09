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
    /// SMPTE-style 75% color bars.
    #[default]
    SmpteColorBars,
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

/// Fill a UYVY buffer with the requested pattern.
///
/// `phase` in `0..1` enables animation (scroll / moving marker) when meaningful.
pub fn fill_uyvy(kind: PatternKind, dst: &mut [u8], width: i32, height: i32, phase: f32) {
    let w = width.max(2) as usize;
    let h = height.max(1) as usize;
    let stride = w * 2;
    assert!(dst.len() >= stride * h);

    match kind {
        PatternKind::SmpteColorBars => fill_bars(dst, w, h, stride, phase),
        PatternKind::Black => fill_solid(dst, w, h, stride, 0, 0, 0),
        PatternKind::White => fill_solid(dst, w, h, stride, 235, 235, 235),
        PatternKind::Gray => fill_solid(dst, w, h, stride, 128, 128, 128),
        PatternKind::Grid => fill_grid(dst, w, h, stride, phase),
        PatternKind::Ramp => fill_ramp(dst, w, h, stride, phase),
        PatternKind::Image => fill_solid(dst, w, h, stride, 16, 16, 16),
    }
}

/// Allocate and fill a UYVY frame.
pub fn generate_uyvy(kind: PatternKind, width: i32, height: i32, phase: f32) -> Vec<u8> {
    let mut buf = vec![0u8; (width.max(2) as usize) * 2 * (height.max(1) as usize)];
    fill_uyvy(kind, &mut buf, width, height, phase);
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

fn fill_bars(dst: &mut [u8], w: usize, h: usize, stride: usize, phase: f32) {
    let offset = if phase == 0.0 {
        0
    } else {
        ((phase.rem_euclid(1.0)) * w as f32) as usize
    };
    let mut line = vec![0u8; stride];
    for x in (0..w).step_by(2) {
        let x0 = (x + offset) % w;
        let x1 = (x + 1 + offset) % w;
        let (y0, u0, v0) = rgb_to_uyvy_pixel_tuple(bar_at(x0, w));
        let (y1, u1, v1) = rgb_to_uyvy_pixel_tuple(bar_at(x1, w));
        let u = ((u0 as u16 + u1 as u16) / 2) as u8;
        let v = ((v0 as u16 + v1 as u16) / 2) as u8;
        let o = x * 2;
        line[o] = u;
        line[o + 1] = y0;
        line[o + 2] = v;
        line[o + 3] = y1;
    }
    for y in 0..h {
        dst[y * stride..(y + 1) * stride].copy_from_slice(&line);
    }
    if phase != 0.0 {
        let my = ((phase.rem_euclid(1.0)) * h as f32) as usize % h;
        let row = &mut dst[my * stride..(my + 1) * stride];
        for x in (0..w).step_by(2) {
            let o = x * 2;
            row[o + 1] = 235;
            row[o + 3] = 235;
        }
    }
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

fn fill_grid(dst: &mut [u8], w: usize, h: usize, stride: usize, phase: f32) {
    let spacing = 64usize;
    let shift = ((phase.rem_euclid(1.0)) * spacing as f32) as usize;
    for y in 0..h {
        for x in (0..w).step_by(2) {
            let on = ((x + shift) % spacing == 0)
                || ((x + 1 + shift) % spacing == 0)
                || ((y + shift) % spacing == 0);
            let (r, g, b) = if on { (235, 235, 235) } else { (16, 16, 16) };
            let (y0, u0, v0) = rgb_to_uyvy_pixel(r, g, b);
            let (y1, u1, v1) = rgb_to_uyvy_pixel(r, g, b);
            let u = ((u0 as u16 + u1 as u16) / 2) as u8;
            let v = ((v0 as u16 + v1 as u16) / 2) as u8;
            let o = y * stride + x * 2;
            dst[o] = u;
            dst[o + 1] = y0;
            dst[o + 2] = v;
            dst[o + 3] = y1;
        }
    }
}

fn fill_ramp(dst: &mut [u8], w: usize, h: usize, stride: usize, phase: f32) {
    let shift = ((phase.rem_euclid(1.0)) * w as f32) as usize;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bars_have_expected_size() {
        let frame = generate_uyvy(PatternKind::SmpteColorBars, 64, 36, 0.0);
        assert_eq!(frame.len(), 64 * 2 * 36);
        // Leftmost bar should be near-white luma.
        assert!(frame[1] > 150);
    }

    #[test]
    fn black_is_studio_black() {
        let frame = generate_uyvy(PatternKind::Black, 16, 16, 0.0);
        assert_eq!(frame[1], 16);
    }

    #[test]
    fn all_builtins_fill() {
        for kind in PatternKind::builtins() {
            let _ = generate_uyvy(*kind, 32, 18, 0.25);
        }
    }
}

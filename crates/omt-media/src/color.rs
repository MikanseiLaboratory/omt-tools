//! Color / pixel format helpers.

/// Convert BGRA8 to RGBA8 for egui textures.
pub fn bgra_to_rgba(bgra: &[u8]) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(bgra.len());
    for px in bgra.chunks_exact(4) {
        rgba.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
    }
    rgba
}

/// Replace RGB with grayscale alpha visualization (A→RGB, A=255).
pub fn bgra_alpha_mask(bgra: &[u8]) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(bgra.len());
    for px in bgra.chunks_exact(4) {
        let a = px[3];
        rgba.extend_from_slice(&[a, a, a, 255]);
    }
    rgba
}

/// Composite BGRA over a checkerboard background into RGBA.
pub fn bgra_over_checkerboard(bgra: &[u8], width: u32, height: u32, cell: u32) -> Vec<u8> {
    let cell = cell.max(1);
    let mut rgba = Vec::with_capacity(bgra.len());
    for y in 0..height {
        for x in 0..width {
            let i = ((y * width + x) * 4) as usize;
            let (b, g, r, a) = (bgra[i], bgra[i + 1], bgra[i + 2], bgra[i + 3]);
            let checker = ((x / cell) + (y / cell)) % 2 == 0;
            let (br, bg, bb) = if checker {
                (180u8, 180u8, 180u8)
            } else {
                (80u8, 80u8, 80u8)
            };
            let af = a as f32 / 255.0;
            let out_r = (r as f32 * af + br as f32 * (1.0 - af)).round() as u8;
            let out_g = (g as f32 * af + bg as f32 * (1.0 - af)).round() as u8;
            let out_b = (b as f32 * af + bb as f32 * (1.0 - af)).round() as u8;
            rgba.extend_from_slice(&[out_r, out_g, out_b, 255]);
        }
    }
    rgba
}

/// Convert one RGB pixel to BT.709 studio-range YUV.
pub fn rgb_to_uyvy_pixel(r: u8, g: u8, b: u8) -> (u8, u8, u8) {
    let (r, g, b) = (r as i32, g as i32, b as i32);
    let y = ((66 * r + 129 * g + 25 * b + 128) >> 8) + 16;
    let u = ((-38 * r - 74 * g + 112 * b + 128) >> 8) + 128;
    let v = ((112 * r - 94 * g - 18 * b + 128) >> 8) + 128;
    (
        y.clamp(16, 235) as u8,
        u.clamp(16, 240) as u8,
        v.clamp(16, 240) as u8,
    )
}

/// Pack an RGB24 tightly packed frame into UYVY.
pub fn uyvy_from_rgb_frame(rgb: &[u8], width: u32, height: u32) -> Vec<u8> {
    let w = width as usize;
    let h = height as usize;
    let mut uyvy = vec![0u8; w * 2 * h];
    for y in 0..h {
        for x in (0..w).step_by(2) {
            let i0 = (y * w + x) * 3;
            let i1 = (y * w + x + 1).min(y * w + w - 1) * 3;
            let (y0, u0, v0) = rgb_to_uyvy_pixel(rgb[i0], rgb[i0 + 1], rgb[i0 + 2]);
            let (y1, u1, v1) = rgb_to_uyvy_pixel(rgb[i1], rgb[i1 + 1], rgb[i1 + 2]);
            let u = ((u0 as u16 + u1 as u16) / 2) as u8;
            let v = ((v0 as u16 + v1 as u16) / 2) as u8;
            let o = y * w * 2 + x * 2;
            uyvy[o] = u;
            uyvy[o + 1] = y0;
            uyvy[o + 2] = v;
            uyvy[o + 3] = y1;
        }
    }
    uyvy
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bgra_to_rgba_swaps_channels() {
        let bgra = [10u8, 20, 30, 255];
        assert_eq!(bgra_to_rgba(&bgra), vec![30, 20, 10, 255]);
    }

    #[test]
    fn alpha_mask_uses_alpha() {
        let bgra = [1u8, 2, 3, 128];
        assert_eq!(bgra_alpha_mask(&bgra), vec![128, 128, 128, 255]);
    }
}

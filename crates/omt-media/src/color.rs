//! Send-side pixel helpers (RGB → UYVY) for pattern / capture paths.

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
    fn uyvy_from_rgb_black() {
        let rgb = [0u8, 0, 0, 0, 0, 0];
        let uyvy = uyvy_from_rgb_frame(&rgb, 2, 1);
        assert_eq!(uyvy.len(), 4);
        assert_eq!(uyvy[1], 16);
        assert_eq!(uyvy[3], 16);
    }
}

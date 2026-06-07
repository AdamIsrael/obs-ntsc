// YUV ↔ packed RGBA roundtrip, using the per-frame ColorMatrix OBS provides.
// All conversion is into/out of a packed RGBA scratch buffer (alpha = 255)
// that the caller owns and reuses across frames.

use crate::colormatrix::ColorMatrix;

/// UYVY → packed RGBA (alpha = 255).
pub fn uyvy_to_rgba(
    src: &[u8],
    src_linesize: usize,
    rgba: &mut [u8],
    width: usize,
    height: usize,
    cm: &ColorMatrix,
) {
    let pair_count = width / 2;
    for y in 0..height {
        let src_row = &src[y * src_linesize..y * src_linesize + pair_count * 4];
        let dst_row = &mut rgba[y * width * 4..y * width * 4 + width * 4];
        for p in 0..pair_count {
            let u = src_row[p * 4];
            let y0 = src_row[p * 4 + 1];
            let v = src_row[p * 4 + 2];
            let y1 = src_row[p * 4 + 3];
            let (r0, g0, b0) = cm.yuv_to_rgb(y0, u, v);
            let (r1, g1, b1) = cm.yuv_to_rgb(y1, u, v);
            dst_row[p * 8] = r0;
            dst_row[p * 8 + 1] = g0;
            dst_row[p * 8 + 2] = b0;
            dst_row[p * 8 + 3] = 255;
            dst_row[p * 8 + 4] = r1;
            dst_row[p * 8 + 5] = g1;
            dst_row[p * 8 + 6] = b1;
            dst_row[p * 8 + 7] = 255;
        }
    }
}

/// Packed RGBA → UYVY. Chroma averaged across each horizontal pixel pair.
pub fn rgba_to_uyvy(
    rgba: &[u8],
    dst: &mut [u8],
    dst_linesize: usize,
    width: usize,
    height: usize,
    cm: &ColorMatrix,
) {
    let pair_count = width / 2;
    for y in 0..height {
        let src_row = &rgba[y * width * 4..y * width * 4 + width * 4];
        let dst_row = &mut dst[y * dst_linesize..y * dst_linesize + pair_count * 4];
        for p in 0..pair_count {
            let r0 = src_row[p * 8];
            let g0 = src_row[p * 8 + 1];
            let b0 = src_row[p * 8 + 2];
            let r1 = src_row[p * 8 + 4];
            let g1 = src_row[p * 8 + 5];
            let b1 = src_row[p * 8 + 6];
            let (y0, u0, v0) = cm.rgb_to_yuv(r0, g0, b0);
            let (y1, u1, v1) = cm.rgb_to_yuv(r1, g1, b1);
            let u = ((u0 as u16 + u1 as u16) / 2) as u8;
            let v = ((v0 as u16 + v1 as u16) / 2) as u8;
            dst_row[p * 4] = u;
            dst_row[p * 4 + 1] = y0;
            dst_row[p * 4 + 2] = v;
            dst_row[p * 4 + 3] = y1;
        }
    }
}

/// NV12 (planar Y + interleaved UV at half-res) → packed RGBA.
pub fn nv12_to_rgba(
    y_plane: &[u8],
    y_linesize: usize,
    uv_plane: &[u8],
    uv_linesize: usize,
    rgba: &mut [u8],
    width: usize,
    height: usize,
    cm: &ColorMatrix,
) {
    for row in 0..height {
        let y_row = &y_plane[row * y_linesize..row * y_linesize + width];
        let uv_row = &uv_plane[(row / 2) * uv_linesize..(row / 2) * uv_linesize + (width / 2) * 2];
        let dst_row = &mut rgba[row * width * 4..row * width * 4 + width * 4];
        for x in 0..width {
            let y = y_row[x];
            let u = uv_row[(x / 2) * 2];
            let v = uv_row[(x / 2) * 2 + 1];
            let (r, g, b) = cm.yuv_to_rgb(y, u, v);
            dst_row[x * 4] = r;
            dst_row[x * 4 + 1] = g;
            dst_row[x * 4 + 2] = b;
            dst_row[x * 4 + 3] = 255;
        }
    }
}

/// Packed RGBA → NV12. Chroma averaged across each 2x2 pixel block.
pub fn rgba_to_nv12(
    rgba: &[u8],
    y_plane: &mut [u8],
    y_linesize: usize,
    uv_plane: &mut [u8],
    uv_linesize: usize,
    width: usize,
    height: usize,
    cm: &ColorMatrix,
) {
    // Full-resolution Y plane.
    for row in 0..height {
        let src_row = &rgba[row * width * 4..row * width * 4 + width * 4];
        let dst_row = &mut y_plane[row * y_linesize..row * y_linesize + width];
        for x in 0..width {
            let r = src_row[x * 4];
            let g = src_row[x * 4 + 1];
            let b = src_row[x * 4 + 2];
            let (y, _, _) = cm.rgb_to_yuv(r, g, b);
            dst_row[x] = y;
        }
    }
    // Interleaved UV plane at half height, averaged over 2x2 blocks.
    let uv_rows = height / 2;
    let pair_count = width / 2;
    for uv_row in 0..uv_rows {
        let top = uv_row * 2;
        let bot = top + 1;
        let top_row = &rgba[top * width * 4..top * width * 4 + width * 4];
        let bot_row = &rgba[bot * width * 4..bot * width * 4 + width * 4];
        let dst = &mut uv_plane[uv_row * uv_linesize..uv_row * uv_linesize + pair_count * 2];
        for p in 0..pair_count {
            let mut u_sum: u32 = 0;
            let mut v_sum: u32 = 0;
            for &row in &[top_row, bot_row] {
                for &xoff in &[0usize, 1] {
                    let x = p * 2 + xoff;
                    let r = row[x * 4];
                    let g = row[x * 4 + 1];
                    let b = row[x * 4 + 2];
                    let (_, u, v) = cm.rgb_to_yuv(r, g, b);
                    u_sum += u as u32;
                    v_sum += v as u32;
                }
            }
            dst[p * 2] = (u_sum / 4) as u8;
            dst[p * 2 + 1] = (v_sum / 4) as u8;
        }
    }
}

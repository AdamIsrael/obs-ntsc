// YUV ↔ RGB driven by the per-frame matrix OBS computes for the source's
// color space and range. Replaces our previous hardcoded BT.601 fallback.
//
// OBS layout: `obs_source_frame.color_matrix: [f32; 16]` is row-major.
// Rows 0/1/2 produce R/G/B from normalized 0-1 YUV (with column 3 as the
// offset for range scaling); row 3 is [0,0,0,1]. For limited-range frames,
// `color_range_min/max` clamp YUV before the matrix is applied.

#[derive(Debug, Clone, Copy)]
pub struct ColorMatrix {
    // Forward: normalized YUV → normalized RGB. m_fwd[row][col].
    m_fwd: [[f32; 4]; 3],
    // Inverse of the 3x3 portion (for RGB → YUV); offset is m_fwd[*][3].
    m_inv: [[f32; 3]; 3],
    range_min: [f32; 3],
    range_max: [f32; 3],
    full_range: bool,
}

impl ColorMatrix {
    pub fn from_obs(
        matrix: &[f32; 16],
        range_min: &[f32; 3],
        range_max: &[f32; 3],
        full_range: bool,
    ) -> Self {
        let m_fwd = [
            [matrix[0], matrix[1], matrix[2], matrix[3]],
            [matrix[4], matrix[5], matrix[6], matrix[7]],
            [matrix[8], matrix[9], matrix[10], matrix[11]],
        ];
        let m_inv = invert_3x3([
            [m_fwd[0][0], m_fwd[0][1], m_fwd[0][2]],
            [m_fwd[1][0], m_fwd[1][1], m_fwd[1][2]],
            [m_fwd[2][0], m_fwd[2][1], m_fwd[2][2]],
        ]);
        Self {
            m_fwd,
            m_inv,
            range_min: *range_min,
            range_max: *range_max,
            full_range,
        }
    }

    #[inline(always)]
    pub fn yuv_to_rgb(&self, y: u8, u: u8, v: u8) -> (u8, u8, u8) {
        let mut yn = y as f32 / 255.0;
        let mut un = u as f32 / 255.0;
        let mut vn = v as f32 / 255.0;
        if !self.full_range {
            yn = yn.clamp(self.range_min[0], self.range_max[0]);
            un = un.clamp(self.range_min[1], self.range_max[1]);
            vn = vn.clamp(self.range_min[2], self.range_max[2]);
        }
        let r = self.m_fwd[0][0] * yn + self.m_fwd[0][1] * un + self.m_fwd[0][2] * vn + self.m_fwd[0][3];
        let g = self.m_fwd[1][0] * yn + self.m_fwd[1][1] * un + self.m_fwd[1][2] * vn + self.m_fwd[1][3];
        let b = self.m_fwd[2][0] * yn + self.m_fwd[2][1] * un + self.m_fwd[2][2] * vn + self.m_fwd[2][3];
        (
            (r * 255.0).clamp(0.0, 255.0) as u8,
            (g * 255.0).clamp(0.0, 255.0) as u8,
            (b * 255.0).clamp(0.0, 255.0) as u8,
        )
    }

    #[inline(always)]
    pub fn rgb_to_yuv(&self, r: u8, g: u8, b: u8) -> (u8, u8, u8) {
        // Inverse: yuv = A^-1 * (rgb - t), where t = m_fwd[*][3].
        let rn = r as f32 / 255.0 - self.m_fwd[0][3];
        let gn = g as f32 / 255.0 - self.m_fwd[1][3];
        let bn = b as f32 / 255.0 - self.m_fwd[2][3];
        let yn = self.m_inv[0][0] * rn + self.m_inv[0][1] * gn + self.m_inv[0][2] * bn;
        let un = self.m_inv[1][0] * rn + self.m_inv[1][1] * gn + self.m_inv[1][2] * bn;
        let vn = self.m_inv[2][0] * rn + self.m_inv[2][1] * gn + self.m_inv[2][2] * bn;
        (
            (yn * 255.0).clamp(0.0, 255.0) as u8,
            (un * 255.0).clamp(0.0, 255.0) as u8,
            (vn * 255.0).clamp(0.0, 255.0) as u8,
        )
    }
}

fn invert_3x3(m: [[f32; 3]; 3]) -> [[f32; 3]; 3] {
    let a = m[0][0]; let b = m[0][1]; let c = m[0][2];
    let d = m[1][0]; let e = m[1][1]; let f = m[1][2];
    let g = m[2][0]; let h = m[2][1]; let i = m[2][2];

    let det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
    // OBS's YUV→RGB matrices are always invertible; fall back to identity if
    // we somehow see a singular one rather than producing NaNs.
    if det.abs() < 1e-10 {
        return [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    }
    let inv_det = 1.0 / det;

    [
        [
            (e * i - f * h) * inv_det,
            (c * h - b * i) * inv_det,
            (b * f - c * e) * inv_det,
        ],
        [
            (f * g - d * i) * inv_det,
            (a * i - c * g) * inv_det,
            (c * d - a * f) * inv_det,
        ],
        [
            (d * h - e * g) * inv_det,
            (b * g - a * h) * inv_det,
            (a * e - b * d) * inv_det,
        ],
    ]
}

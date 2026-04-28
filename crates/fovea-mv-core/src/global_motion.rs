//! Global motion estimation from frame-wide median MV.
//!
//! Used by [`crate::triggers::MotionTrigger`] to subtract camera motion before
//! computing object-local energy. Reduces false positives from vibration, wind
//! shake, and slow PTZ drift.
//!
//! All displacements are in **quarter-pel (q4) units** throughout:
//! `q4 = motion_x * 4 / motion_scale`. H.264 quarter-pel (`motion_scale = 4`)
//! → identity. HEVC eighth-pel (`motion_scale = 8`) → halved.

use crate::MotionVector;

/// Convert `motion_x` / `motion_y` to quarter-pel units.
///
/// H.264: `motion_scale = 4`, result equals `motion`.
/// HEVC:  `motion_scale = 8`, result equals `motion / 2`.
#[inline]
pub(crate) fn to_q4(motion: i32, scale: u16) -> i32 {
    motion * 4 / scale.max(1) as i32
}

/// Global motion estimate derived from the frame-wide median MV.
#[derive(Debug, Clone, Copy)]
pub struct GlobalMotionEstimate {
    /// Median horizontal displacement, quarter-pel units.
    pub dx_q4: i16,
    /// Median vertical displacement, quarter-pel units.
    pub dy_q4: i16,
    /// Mean absolute deviation of per-MV L1 residuals after global subtraction.
    /// High → heterogeneous motion field → estimate may be unreliable.
    pub residual_mad: f32,
}

/// Estimates camera motion from the dominant (median) MV.
///
/// Component-wise median: O(n log n) sort, two temporary `Vec<i32>` of length
/// `mvs.len()`. Called once per packet, not per MV.
pub struct GlobalMotionEstimator;

impl GlobalMotionEstimator {
    /// Estimate global motion. Returns `None` for empty slices (I-frames).
    pub fn estimate(&self, mvs: &[MotionVector]) -> Option<GlobalMotionEstimate> {
        if mvs.is_empty() {
            return None;
        }

        let mut xs: Vec<i32> =
            mvs.iter().map(|mv| to_q4(mv.motion_x, mv.motion_scale)).collect();
        let mut ys: Vec<i32> =
            mvs.iter().map(|mv| to_q4(mv.motion_y, mv.motion_scale)).collect();
        xs.sort_unstable();
        ys.sort_unstable();

        let mid = xs.len() / 2;
        let dx = if xs.len() % 2 == 0 { (xs[mid - 1] + xs[mid]) / 2 } else { xs[mid] };
        let dy = if ys.len() % 2 == 0 { (ys[mid - 1] + ys[mid]) / 2 } else { ys[mid] };

        let sum_mad: i64 = mvs
            .iter()
            .map(|mv| {
                let rx = to_q4(mv.motion_x, mv.motion_scale) - dx;
                let ry = to_q4(mv.motion_y, mv.motion_scale) - dy;
                (rx.abs() + ry.abs()) as i64
            })
            .sum();
        let residual_mad = sum_mad as f32 / mvs.len() as f32;

        Some(GlobalMotionEstimate {
            dx_q4: dx.clamp(i16::MIN as i32, i16::MAX as i32) as i16,
            dy_q4: dy.clamp(i16::MIN as i32, i16::MAX as i32) as i16,
            residual_mad,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MotionVector;

    fn mv_uniform(mx: i32, my: i32) -> MotionVector {
        MotionVector {
            w: 16,
            h: 16,
            src_x: 0,
            src_y: 0,
            dst_x: 0,
            dst_y: 0,
            motion_x: mx,
            motion_y: my,
            motion_scale: 4,
            source: -1,
        }
    }

    #[test]
    fn estimate_returns_none_for_empty() {
        assert!(GlobalMotionEstimator.estimate(&[]).is_none());
    }

    #[test]
    fn estimate_pure_translation_odd_count() {
        // 3 MVs all at (8, -4) q4 → median = (8, -4)
        let mvs = vec![mv_uniform(8, -4); 3];
        let g = GlobalMotionEstimator.estimate(&mvs).unwrap();
        assert_eq!(g.dx_q4, 8);
        assert_eq!(g.dy_q4, -4);
        assert_eq!(g.residual_mad, 0.0);
    }

    #[test]
    fn estimate_median_even_count() {
        // xs = [2, 4, 6, 8] → median = (4+6)/2 = 5
        let mvs = vec![
            mv_uniform(2, 0),
            mv_uniform(4, 0),
            mv_uniform(6, 0),
            mv_uniform(8, 0),
        ];
        let g = GlobalMotionEstimator.estimate(&mvs).unwrap();
        assert_eq!(g.dx_q4, 5);
    }

    #[test]
    fn estimate_residual_mad_nonzero_for_mixed() {
        // One outlier at (40, 0) among uniform (4, 0) MVs
        let mut mvs = vec![mv_uniform(4, 0); 9];
        mvs.push(mv_uniform(40, 0));
        let g = GlobalMotionEstimator.estimate(&mvs).unwrap();
        // Median dx = 4 (9 copies dominate)
        assert_eq!(g.dx_q4, 4);
        // MAD > 0 because of the outlier
        assert!(g.residual_mad > 0.0);
    }

    #[test]
    fn to_q4_h264_identity() {
        // H.264: motion_scale = 4 → q4 = motion
        assert_eq!(to_q4(12, 4), 12);
        assert_eq!(to_q4(-8, 4), -8);
    }

    #[test]
    fn to_q4_hevc_halved() {
        // HEVC: motion_scale = 8 → q4 = motion / 2
        assert_eq!(to_q4(16, 8), 8);
        assert_eq!(to_q4(-8, 8), -4);
    }
}

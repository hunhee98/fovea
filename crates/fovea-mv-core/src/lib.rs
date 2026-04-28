//! fovea-mv-core
//!
//! H.264 motion-vector parsing and trigger primitives.
//!
//! Status: scaffolding. See `docs/05.exec-plans/001-mvtrigger-mvp.md`.

#![warn(missing_docs)]

/// Placeholder type for a parsed motion vector.
///
/// Real implementation lands in MVP step 2 (see exec-plan 001).
#[derive(Debug, Clone, Copy)]
pub struct MotionVector {
    /// Macroblock x in pixels.
    pub x: u16,
    /// Macroblock y in pixels.
    pub y: u16,
    /// Horizontal displacement (quarter-pel units in H.264; we will normalize).
    pub dx: i16,
    /// Vertical displacement.
    pub dy: i16,
}

/// Aggregate motion energy across a set of motion vectors.
///
/// Energy is the sum of L1 magnitudes. Cheap, allocation-free.
pub fn motion_energy(mvs: &[MotionVector]) -> u64 {
    let mut sum: u64 = 0;
    for mv in mvs {
        sum += (mv.dx.unsigned_abs() as u64) + (mv.dy.unsigned_abs() as u64);
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn motion_energy_zero_for_empty() {
        assert_eq!(motion_energy(&[]), 0);
    }

    #[test]
    fn motion_energy_sums_l1_magnitudes() {
        let mvs = [
            MotionVector { x: 0, y: 0, dx: 3, dy: -4 },
            MotionVector { x: 16, y: 0, dx: -1, dy: 2 },
        ];
        assert_eq!(motion_energy(&mvs), 3 + 4 + 1 + 2);
    }
}

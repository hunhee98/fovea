//! fovea-mv-core
//!
//! Stable types and arithmetic primitives for the fovea-mv subproject.
//!
//! Scope (deliberate):
//! - Plain data types: [`MotionVector`], [`MvPacket`], [`Event`], [`FrameType`].
//! - Cheap, allocation-free aggregations: [`motion_energy`], [`intra_ratio`].
//! - The [`Trigger`] trait and three implementations under [`triggers`]:
//!   [`triggers::MotionTrigger`], [`triggers::IntervalTrigger`],
//!   [`triggers::SceneChangeTrigger`].
//!
//! Out of scope:
//! - H.264 bitstream parsing — lives in `fovea-mv-stream` behind `ffmpeg-next`.
//! - Pixel decoding — `fovea-mv-stream` exposes a separate path.
//! - AI runtime dependencies — never in core.

#![warn(missing_docs)]

pub mod global_motion;
pub mod triggers;

use std::fmt;

/// Frame type for a single packet.
///
/// Mirrors H.264 / MPEG slice types collapsed to the picture-level kind that
/// triggers care about. SI/SP collapse to I, SP also to P.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FrameType {
    /// Intra-coded frame (I-frame). No motion vectors are encoded.
    I,
    /// Predicted frame (P-frame). Forward motion vectors only.
    P,
    /// Bi-predicted frame (B-frame). Forward + backward motion vectors.
    B,
    /// Other / unknown picture type.
    Other,
}

impl FrameType {
    /// Single-character tag matching FFmpeg's `av_get_picture_type_char` for I/P/B.
    pub fn as_char(self) -> char {
        match self {
            Self::I => 'I',
            Self::P => 'P',
            Self::B => 'B',
            Self::Other => '?',
        }
    }
}

impl fmt::Display for FrameType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::I => "I",
            Self::P => "P",
            Self::B => "B",
            Self::Other => "Other",
        })
    }
}

/// One motion vector covering a block of pixels.
///
/// Field semantics follow FFmpeg's `AVMotionVector`:
/// - `(src_x, src_y)`: where in the reference frame the block was copied from.
/// - `(dst_x, dst_y)`: where in the current frame the block lands.
/// - `motion_x` / `motion_y`: integer displacement in `1 / motion_scale` pixel units.
/// - `w × h`: block size in pixels (16, 8, 4, ...).
/// - `source`: reference direction. -1 = past (forward), 1 = future (backward).
#[derive(Debug, Clone, Copy)]
pub struct MotionVector {
    /// Block width in pixels.
    pub w: u8,
    /// Block height in pixels.
    pub h: u8,
    /// Source x in pixels.
    pub src_x: i16,
    /// Source y in pixels.
    pub src_y: i16,
    /// Destination x in pixels.
    pub dst_x: i16,
    /// Destination y in pixels.
    pub dst_y: i16,
    /// Horizontal motion in 1/motion_scale pixel units.
    pub motion_x: i32,
    /// Vertical motion.
    pub motion_y: i32,
    /// Divisor for `motion_x` / `motion_y` (typically 4 for quarter-pel).
    pub motion_scale: u16,
    /// Reference direction: -1 = past (forward), 1 = future (backward).
    pub source: i8,
}

impl MotionVector {
    /// L1 magnitude in 1/motion_scale pixel units. Allocation-free.
    #[inline]
    pub fn l1_magnitude(&self) -> u64 {
        (self.motion_x.unsigned_abs() as u64) + (self.motion_y.unsigned_abs() as u64)
    }
}

/// Per-packet motion-vector summary.
///
/// `mvs` is owned. Producers ([`fovea-mv-stream`]) should reuse the buffer
/// across packets via [`MvPacket::clear`] and `Vec::extend`, not allocate fresh.
#[derive(Debug, Clone)]
pub struct MvPacket {
    /// Presentation timestamp in microseconds since stream start.
    /// Negative = unknown (FFmpeg `AV_NOPTS_VALUE`).
    pub ts_us: i64,
    /// Picture type of this packet.
    pub frame_type: FrameType,
    /// Total macroblock count for this picture (typically `ceil(w/16) * ceil(h/16)`).
    pub total_mb: u32,
    /// Macroblocks coded as intra. `intra_count == total_mb` for I-frames.
    pub intra_count: u32,
    /// Macroblocks coded as MODE_SKIP. Currently populated only by the
    /// HEVC path (via the libde265 `de265_internals_get_CB_stats`
    /// accessor). H.264 leaves this at 0 — `+export_mvs` does not
    /// expose skip information. A high `skip_count / total_mb` is the
    /// encoder's explicit "this region didn't change" signal, useful
    /// as an idle confidence boost for triggers.
    pub skip_count: u32,
    /// All extracted motion vectors. Empty for I-frames.
    pub mvs: Vec<MotionVector>,
}

impl MvPacket {
    /// Empty packet. Use [`MvPacket::clear`] for buffer-reusing reset.
    pub fn empty() -> Self {
        Self {
            ts_us: 0,
            frame_type: FrameType::Other,
            total_mb: 0,
            intra_count: 0,
            skip_count: 0,
            mvs: Vec::new(),
        }
    }

    /// Reset state, retain `mvs` capacity.
    pub fn clear(&mut self) {
        self.ts_us = 0;
        self.frame_type = FrameType::Other;
        self.total_mb = 0;
        self.intra_count = 0;
        self.skip_count = 0;
        self.mvs.clear();
    }
}

/// Triggered event.
///
/// Carries metadata downstream consumers use to decide whether to do
/// something expensive (decode RGB, call a VLM). Pixel data is fetched
/// separately via the source's lazy decode path.
#[derive(Debug, Clone)]
pub struct Event {
    /// Same as [`MvPacket::ts_us`].
    pub ts_us: i64,
    /// Picture type when the trigger fired.
    pub frame_type: FrameType,
    /// Stable identifier (e.g. `"motion"`, `"interval"`, `"scene_change"`).
    pub trigger_name: &'static str,
    /// Motion energy at fire time (sum of L1 magnitudes / motion_scale).
    pub energy: u64,
    /// Fraction of macroblocks coded as intra in the firing packet (0.0..=1.0).
    pub intra_ratio: f32,
    /// Fraction of macroblocks coded as MODE_SKIP in the firing packet
    /// (0.0..=1.0). Populated for HEVC; always 0.0 for H.264 today.
    pub skip_ratio: f32,
    /// Motion vector count in the firing packet.
    pub mv_count: u32,
}

/// Trigger trait. Stateful (sliding windows, last-fire timestamps, etc.).
pub trait Trigger {
    /// Evaluate one packet. Returns `Some(Event)` if the trigger fired.
    fn evaluate(&mut self, packet: &MvPacket) -> Option<Event>;

    /// Stable identifier surfaced via [`Event::trigger_name`].
    fn name(&self) -> &'static str;
}

/// Sum of L1 motion magnitudes, normalized by each vector's `motion_scale`.
///
/// Integer floor. No allocation, no float in hot path.
#[inline]
pub fn motion_energy(mvs: &[MotionVector]) -> u64 {
    let mut sum: u64 = 0;
    for mv in mvs {
        let scale = mv.motion_scale.max(1) as u64;
        sum += mv.l1_magnitude() / scale;
    }
    sum
}

/// Ratio of intra-coded macroblocks. 0.0 when `total_mb == 0`.
#[inline]
pub fn intra_ratio(packet: &MvPacket) -> f32 {
    if packet.total_mb == 0 {
        0.0
    } else {
        packet.intra_count as f32 / packet.total_mb as f32
    }
}

/// Ratio of MODE_SKIP macroblocks. 0.0 when `total_mb == 0` or when
/// the source pipeline does not populate `skip_count` (H.264 today).
#[inline]
pub fn skip_ratio(packet: &MvPacket) -> f32 {
    if packet.total_mb == 0 {
        0.0
    } else {
        packet.skip_count as f32 / packet.total_mb as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mv(mx: i32, my: i32, scale: u16) -> MotionVector {
        MotionVector {
            w: 16,
            h: 16,
            src_x: 0,
            src_y: 0,
            dst_x: 0,
            dst_y: 0,
            motion_x: mx,
            motion_y: my,
            motion_scale: scale,
            source: -1,
        }
    }

    #[test]
    fn motion_energy_zero_for_empty() {
        assert_eq!(motion_energy(&[]), 0);
    }

    #[test]
    fn motion_energy_sums_normalized_magnitudes() {
        // |3| + |-4| = 7, scale=1 → 7
        // |-8| + |2| = 10, scale=4 → 2 (floor)
        let mvs = [mv(3, -4, 1), mv(-8, 2, 4)];
        assert_eq!(motion_energy(&mvs), 7 + 2);
    }

    #[test]
    fn l1_magnitude_basic() {
        assert_eq!(mv(3, -4, 1).l1_magnitude(), 7);
        assert_eq!(mv(0, 0, 1).l1_magnitude(), 0);
    }

    #[test]
    fn intra_ratio_handles_empty() {
        let packet = MvPacket::empty();
        assert_eq!(intra_ratio(&packet), 0.0);
    }

    #[test]
    fn intra_ratio_basic() {
        let packet = MvPacket {
            ts_us: 0,
            frame_type: FrameType::P,
            total_mb: 100,
            intra_count: 25,
            skip_count: 0,
            mvs: vec![],
        };
        assert!((intra_ratio(&packet) - 0.25).abs() < 1e-6);
    }

    #[test]
    fn skip_ratio_basic() {
        let packet = MvPacket {
            ts_us: 0,
            frame_type: FrameType::P,
            total_mb: 100,
            intra_count: 0,
            skip_count: 80,
            mvs: vec![],
        };
        assert!((skip_ratio(&packet) - 0.80).abs() < 1e-6);
    }

    #[test]
    fn skip_ratio_handles_empty() {
        let packet = MvPacket::empty();
        assert_eq!(skip_ratio(&packet), 0.0);
    }

    #[test]
    fn mv_packet_clear_retains_capacity() {
        let mut p = MvPacket {
            ts_us: 12345,
            frame_type: FrameType::P,
            total_mb: 100,
            intra_count: 5,
            skip_count: 60,
            mvs: vec![mv(1, 2, 4); 50],
        };
        let cap = p.mvs.capacity();
        p.clear();
        assert_eq!(p.ts_us, 0);
        assert_eq!(p.frame_type, FrameType::Other);
        assert_eq!(p.total_mb, 0);
        assert_eq!(p.intra_count, 0);
        assert_eq!(p.skip_count, 0);
        assert!(p.mvs.is_empty());
        assert_eq!(p.mvs.capacity(), cap);
    }

    #[test]
    fn frame_type_char() {
        assert_eq!(FrameType::I.as_char(), 'I');
        assert_eq!(FrameType::P.as_char(), 'P');
        assert_eq!(FrameType::B.as_char(), 'B');
        assert_eq!(FrameType::Other.as_char(), '?');
    }
}

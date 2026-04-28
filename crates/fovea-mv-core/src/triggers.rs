//! Trigger implementations.
//!
//! Three primitives, each implementing [`crate::Trigger`]:
//! - [`MotionTrigger`] — fires when motion energy stays above a threshold
//!   for a minimum duration.
//! - [`IntervalTrigger`] — heartbeat; fires at least once per `max_gap_ms`.
//! - [`SceneChangeTrigger`] — fires on inter frames whose intra ratio crosses
//!   a threshold (e.g. encoder-inserted intra refresh on a scene change).
//!
//! All triggers are stateful and operate on a single packet at a time.
//! Composing multiple triggers is the caller's responsibility — the order in
//! which they are evaluated determines which `Event` wins for a given packet.

use crate::{intra_ratio, motion_energy, skip_ratio, Event, FrameType, MotionVector, MvPacket, Trigger};

/// Rectangular region of interest, used by [`MotionTrigger`] to ignore motion
/// outside a fixed area of the frame.
///
/// Coordinates are in pixels, half-open: a vector with `dst_x ∈ [x0, x1)`
/// and `dst_y ∈ [y0, y1)` is considered inside.
#[derive(Debug, Clone, Copy)]
pub struct RegionMask {
    /// Left pixel coordinate (inclusive).
    pub x0: i16,
    /// Top pixel coordinate (inclusive).
    pub y0: i16,
    /// Right pixel coordinate (exclusive).
    pub x1: i16,
    /// Bottom pixel coordinate (exclusive).
    pub y1: i16,
}

impl RegionMask {
    /// Returns `true` if `(dst_x, dst_y)` is inside this region.
    #[inline]
    pub fn contains(&self, dst_x: i16, dst_y: i16) -> bool {
        dst_x >= self.x0 && dst_x < self.x1 && dst_y >= self.y0 && dst_y < self.y1
    }
}

/// How [`MotionTrigger`] interprets its threshold.
///
/// Absolute thresholds are simple but scale with frame area: a value tuned
/// for a 1080p source will not fire at all on a 360p downscale of the same
/// content. Per-MB thresholds normalize by macroblock count and stay valid
/// across resolutions.
#[derive(Debug, Clone, Copy)]
pub enum ThresholdMode {
    /// Sum of L1 magnitudes / motion_scale, summed over all (or masked) MVs.
    /// Direct interpretation of [`motion_energy`].
    Absolute(u64),
    /// `motion_energy / total_mb`, in units of "energy per macroblock".
    /// Resolution-independent. Floored to integer when comparing.
    PerMb(f32),
}

/// Motion-energy trigger.
///
/// Fires once per "above-threshold episode": the first packet whose energy
/// has been at-or-above the threshold continuously for at least
/// `min_duration_ms` produces an event. The trigger then re-arms only after
/// energy drops back below the threshold.
///
/// If `mask` is set, only motion vectors whose destination falls inside the
/// region contribute to the energy. Useful for ignoring fixed moving zones
/// (e.g. a flag, a clock).
pub struct MotionTrigger {
    /// Threshold and the mode in which it is compared.
    pub threshold: ThresholdMode,
    /// Minimum continuous time above threshold before firing, in
    /// milliseconds.
    pub min_duration_ms: u32,
    /// Optional region of interest.
    pub mask: Option<RegionMask>,
    above_since_us: Option<i64>,
    fired_this_episode: bool,
}

impl MotionTrigger {
    /// Construct with an absolute energy threshold (frame-area dependent).
    pub fn new(energy_threshold: u64) -> Self {
        Self::with_threshold(ThresholdMode::Absolute(energy_threshold))
    }

    /// Construct with a per-macroblock energy threshold (resolution-independent).
    ///
    /// Convert from an absolute `T` known to work on a `W × H` clip:
    /// `per_mb = T / ceil(W/16) / ceil(H/16)`.
    pub fn with_per_mb_threshold(threshold_per_mb: f32) -> Self {
        Self::with_threshold(ThresholdMode::PerMb(threshold_per_mb))
    }

    /// Construct with an explicit [`ThresholdMode`].
    pub fn with_threshold(mode: ThresholdMode) -> Self {
        Self {
            threshold: mode,
            min_duration_ms: 0,
            mask: None,
            above_since_us: None,
            fired_this_episode: false,
        }
    }

    /// Set [`Self::min_duration_ms`].
    pub fn with_min_duration_ms(mut self, ms: u32) -> Self {
        self.min_duration_ms = ms;
        self
    }

    /// Set [`Self::mask`].
    pub fn with_mask(mut self, mask: RegionMask) -> Self {
        self.mask = Some(mask);
        self
    }

    fn energy_for(&self, mvs: &[MotionVector]) -> u64 {
        match self.mask {
            None => motion_energy(mvs),
            Some(m) => {
                let mut sum: u64 = 0;
                for mv in mvs {
                    if m.contains(mv.dst_x, mv.dst_y) {
                        let scale = mv.motion_scale.max(1) as u64;
                        sum += mv.l1_magnitude() / scale;
                    }
                }
                sum
            }
        }
    }

    fn absolute_threshold_for(&self, packet: &MvPacket) -> u64 {
        match self.threshold {
            ThresholdMode::Absolute(t) => t,
            ThresholdMode::PerMb(per) => {
                if per <= 0.0 {
                    return 0;
                }
                if packet.total_mb == 0 {
                    // Defensive: an empty `total_mb` means we don't know the
                    // frame area. Refuse to fire rather than fire trivially.
                    return u64::MAX;
                }
                (per * packet.total_mb as f32) as u64
            }
        }
    }
}

impl Trigger for MotionTrigger {
    fn evaluate(&mut self, packet: &MvPacket) -> Option<Event> {
        let energy = self.energy_for(&packet.mvs);
        let threshold = self.absolute_threshold_for(packet);
        if energy >= threshold {
            let started = *self.above_since_us.get_or_insert(packet.ts_us);
            let duration_us = packet.ts_us.saturating_sub(started);
            let needed_us = (self.min_duration_ms as i64) * 1_000;
            if !self.fired_this_episode && duration_us >= needed_us {
                self.fired_this_episode = true;
                return Some(Event {
                    ts_us: packet.ts_us,
                    frame_type: packet.frame_type,
                    trigger_name: self.name(),
                    energy,
                    intra_ratio: intra_ratio(packet),
                    skip_ratio: skip_ratio(packet),
                    mv_count: packet.mvs.len() as u32,
                });
            }
        } else {
            self.above_since_us = None;
            self.fired_this_episode = false;
        }
        None
    }

    fn name(&self) -> &'static str {
        "motion"
    }
}

/// Heartbeat trigger.
///
/// Fires on the first packet, then again whenever `max_gap_ms` has elapsed
/// since the previous fire. Use to ensure downstream consumers see at least
/// one event per window even when nothing else fires.
///
/// This trigger does not look at packet content; it operates on `ts_us`
/// alone.
pub struct IntervalTrigger {
    /// Minimum time between fires.
    pub max_gap_ms: u32,
    last_fire_us: Option<i64>,
}

impl IntervalTrigger {
    /// Construct.
    pub fn new(max_gap_ms: u32) -> Self {
        Self { max_gap_ms, last_fire_us: None }
    }
}

impl Trigger for IntervalTrigger {
    fn evaluate(&mut self, packet: &MvPacket) -> Option<Event> {
        let due = match self.last_fire_us {
            None => true,
            Some(prev) => {
                packet.ts_us.saturating_sub(prev) >= (self.max_gap_ms as i64) * 1_000
            }
        };
        if due {
            self.last_fire_us = Some(packet.ts_us);
            return Some(Event {
                ts_us: packet.ts_us,
                frame_type: packet.frame_type,
                trigger_name: self.name(),
                energy: motion_energy(&packet.mvs),
                intra_ratio: intra_ratio(packet),
                skip_ratio: skip_ratio(packet),
                mv_count: packet.mvs.len() as u32,
            });
        }
        None
    }

    fn name(&self) -> &'static str {
        "interval"
    }
}

/// Scene-change trigger.
///
/// Fires on a P- or B-frame whose `intra_ratio` is at least
/// `intra_block_ratio_threshold`. The H.264 encoder typically inserts a
/// burst of intra-coded macroblocks when a frame's content cannot be
/// predicted from references, which is a strong scene-change signal.
///
/// I-frames are skipped: they always have `intra_ratio == 1.0` by
/// definition, and a regular GOP refresh is not a scene change.
pub struct SceneChangeTrigger {
    /// Threshold in `[0.0, 1.0]`.
    pub intra_block_ratio_threshold: f32,
    last_fire_us: Option<i64>,
}

impl SceneChangeTrigger {
    /// Construct.
    pub fn new(intra_block_ratio_threshold: f32) -> Self {
        Self {
            intra_block_ratio_threshold,
            last_fire_us: None,
        }
    }
}

impl Trigger for SceneChangeTrigger {
    fn evaluate(&mut self, packet: &MvPacket) -> Option<Event> {
        if matches!(packet.frame_type, FrameType::I | FrameType::Other) {
            return None;
        }
        let r = intra_ratio(packet);
        if r >= self.intra_block_ratio_threshold {
            // Avoid double-firing on two adjacent frames carrying the same
            // scene change. Fire at most once every 100 ms.
            if let Some(prev) = self.last_fire_us {
                if packet.ts_us.saturating_sub(prev) < 100_000 {
                    return None;
                }
            }
            self.last_fire_us = Some(packet.ts_us);
            return Some(Event {
                ts_us: packet.ts_us,
                frame_type: packet.frame_type,
                trigger_name: self.name(),
                energy: motion_energy(&packet.mvs),
                intra_ratio: r,
                skip_ratio: skip_ratio(packet),
                mv_count: packet.mvs.len() as u32,
            });
        }
        None
    }

    fn name(&self) -> &'static str {
        "scene_change"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mv(mx: i32, my: i32, dst_x: i16, dst_y: i16) -> MotionVector {
        MotionVector {
            w: 16,
            h: 16,
            src_x: 0,
            src_y: 0,
            dst_x,
            dst_y,
            motion_x: mx,
            motion_y: my,
            motion_scale: 4,
            source: -1,
        }
    }

    fn p_packet(ts_us: i64, mvs: Vec<MotionVector>, intra: u32, total: u32) -> MvPacket {
        MvPacket {
            ts_us,
            frame_type: FrameType::P,
            total_mb: total,
            intra_count: intra,
            skip_count: 0,
            mvs,
        }
    }

    fn i_packet(ts_us: i64, total: u32) -> MvPacket {
        MvPacket {
            ts_us,
            frame_type: FrameType::I,
            total_mb: total,
            intra_count: total,
            skip_count: 0,
            mvs: vec![],
        }
    }

    // ----- MotionTrigger -----

    #[test]
    fn motion_fires_once_per_episode_zero_duration() {
        let mut t = MotionTrigger::new(10);
        // Below threshold first, then above.
        let p1 = p_packet(0, vec![mv(1, 0, 0, 0)], 0, 100);
        let p2 = p_packet(33_000, vec![mv(20, 20, 0, 0)], 0, 100);
        let p3 = p_packet(66_000, vec![mv(20, 20, 0, 0)], 0, 100);
        let p4 = p_packet(100_000, vec![mv(0, 0, 0, 0)], 0, 100);
        assert!(t.evaluate(&p1).is_none());
        let e2 = t.evaluate(&p2);
        assert!(e2.is_some(), "first above-threshold packet should fire");
        // Still above — must not fire again.
        assert!(t.evaluate(&p3).is_none(), "must not refire while above");
        // Drop below — re-arms.
        assert!(t.evaluate(&p4).is_none());
        // Above again — fires.
        let p5 = p_packet(200_000, vec![mv(20, 20, 0, 0)], 0, 100);
        assert!(t.evaluate(&p5).is_some());
    }

    #[test]
    fn motion_respects_min_duration() {
        let mut t = MotionTrigger::new(10).with_min_duration_ms(50);
        // Above at t=0, but min_duration is 50ms.
        let p1 = p_packet(0, vec![mv(20, 20, 0, 0)], 0, 100);
        let p2 = p_packet(20_000, vec![mv(20, 20, 0, 0)], 0, 100); // 20ms — too short
        let p3 = p_packet(60_000, vec![mv(20, 20, 0, 0)], 0, 100); // 60ms — fire
        assert!(t.evaluate(&p1).is_none());
        assert!(t.evaluate(&p2).is_none());
        assert!(t.evaluate(&p3).is_some());
    }

    #[test]
    fn motion_per_mb_threshold_resolution_independent() {
        // Per-MB threshold of 1.0 means: fire when energy >= total_mb.
        // 100 MBs → fires when energy >= 100.
        // 8160 MBs → fires when energy >= 8160.
        // So same trigger config behaves consistently across resolutions.
        let mut trig = MotionTrigger::with_per_mb_threshold(1.0);

        let small = MvPacket {
            ts_us: 0,
            frame_type: FrameType::P,
            total_mb: 100,
            intra_count: 0,
            skip_count: 0,
            mvs: vec![mv(150, 0, 0, 0)], // energy = 150/4 = 37 < 100
        };
        assert!(trig.evaluate(&small).is_none());
        // Now provide enough MVs to exceed 100.
        let mut big_mvs = Vec::new();
        for _ in 0..30 {
            big_mvs.push(mv(20, 20, 0, 0)); // each contributes 40/4 = 10
        }
        let small_active = MvPacket {
            ts_us: 33_000,
            frame_type: FrameType::P,
            total_mb: 100,
            intra_count: 0,
            skip_count: 0,
            mvs: big_mvs,
        };
        // energy = 30 * 10 = 300 >= 100 ✓
        let ev = trig.evaluate(&small_active).expect("must fire");
        assert_eq!(ev.energy, 300);
    }

    #[test]
    fn motion_per_mb_zero_total_does_not_fire() {
        // Defensive: if total_mb == 0 we cannot compute a per-MB threshold,
        // so the trigger refuses to fire rather than fire on every packet.
        let mut trig = MotionTrigger::with_per_mb_threshold(1.0);
        let p = MvPacket {
            ts_us: 0,
            frame_type: FrameType::P,
            total_mb: 0,
            intra_count: 0,
            skip_count: 0,
            mvs: vec![mv(40, 40, 0, 0)],
        };
        assert!(trig.evaluate(&p).is_none());
    }

    #[test]
    fn motion_mask_filters_outside_vectors() {
        let mask = RegionMask {
            x0: 0,
            y0: 0,
            x1: 100,
            y1: 100,
        };
        let mut t = MotionTrigger::new(5).with_mask(mask);
        // Two MVs: one inside mask, one outside. Only inside contributes.
        let inside = mv(20, 20, 50, 50);
        let outside = mv(20, 20, 200, 200);
        let p = p_packet(0, vec![outside], 0, 100);
        assert!(
            t.evaluate(&p).is_none(),
            "outside-mask MV must not fire trigger"
        );
        let p2 = p_packet(33_000, vec![inside], 0, 100);
        assert!(t.evaluate(&p2).is_some());
    }

    // ----- IntervalTrigger -----

    #[test]
    fn interval_fires_first_packet_then_periodic() {
        let mut t = IntervalTrigger::new(100); // 100 ms gap
        let p1 = p_packet(0, vec![], 0, 100);
        let p2 = p_packet(50_000, vec![], 0, 100); // 50ms — too soon
        let p3 = p_packet(120_000, vec![], 0, 100); // 120ms — fire
        let p4 = p_packet(140_000, vec![], 0, 100); // +20ms — too soon
        let p5 = p_packet(230_000, vec![], 0, 100); // +110ms — fire
        assert!(t.evaluate(&p1).is_some());
        assert!(t.evaluate(&p2).is_none());
        assert!(t.evaluate(&p3).is_some());
        assert!(t.evaluate(&p4).is_none());
        assert!(t.evaluate(&p5).is_some());
    }

    // ----- SceneChangeTrigger -----

    #[test]
    fn scene_change_fires_on_high_intra_p_frame() {
        let mut t = SceneChangeTrigger::new(0.4);
        // P-frame with intra_count = 60 / total = 100 → ratio 0.6
        let p = p_packet(0, vec![], 60, 100);
        let ev = t.evaluate(&p).expect("must fire");
        assert!((ev.intra_ratio - 0.6).abs() < 1e-6);
        assert_eq!(ev.trigger_name, "scene_change");
    }

    #[test]
    fn scene_change_skips_i_frames() {
        let mut t = SceneChangeTrigger::new(0.4);
        let p = i_packet(0, 100);
        assert!(t.evaluate(&p).is_none());
    }

    #[test]
    fn scene_change_skips_low_intra() {
        let mut t = SceneChangeTrigger::new(0.4);
        let p = p_packet(0, vec![], 20, 100); // 0.2 < 0.4
        assert!(t.evaluate(&p).is_none());
    }

    #[test]
    fn scene_change_debounces_within_100ms() {
        let mut t = SceneChangeTrigger::new(0.4);
        let a = p_packet(0, vec![], 60, 100);
        let b = p_packet(50_000, vec![], 60, 100);
        let c = p_packet(150_000, vec![], 60, 100);
        assert!(t.evaluate(&a).is_some());
        assert!(t.evaluate(&b).is_none()); // within 100ms
        assert!(t.evaluate(&c).is_some()); // > 100ms later
    }
}

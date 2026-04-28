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

use crate::{intra_ratio, motion_energy, Event, FrameType, MotionVector, MvPacket, Trigger};

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

/// Motion-energy trigger.
///
/// Fires once per "above-threshold episode": the first packet whose energy
/// has been at-or-above `energy_threshold` continuously for at least
/// `min_duration_ms` produces an event. The trigger then re-arms only after
/// energy drops back below the threshold.
///
/// If `mask` is set, only motion vectors whose destination falls inside the
/// region contribute to the energy. Useful for ignoring fixed moving zones
/// (e.g. a flag, a clock).
pub struct MotionTrigger {
    /// L1-magnitude threshold (units: pixels — see [`motion_energy`]).
    pub energy_threshold: u64,
    /// Minimum continuous time above threshold before firing, in
    /// milliseconds.
    pub min_duration_ms: u32,
    /// Optional region of interest.
    pub mask: Option<RegionMask>,
    above_since_us: Option<i64>,
    fired_this_episode: bool,
}

impl MotionTrigger {
    /// Construct with no region mask and zero minimum duration.
    pub fn new(energy_threshold: u64) -> Self {
        Self {
            energy_threshold,
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
}

impl Trigger for MotionTrigger {
    fn evaluate(&mut self, packet: &MvPacket) -> Option<Event> {
        let energy = self.energy_for(&packet.mvs);
        if energy >= self.energy_threshold {
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
            mvs,
        }
    }

    fn i_packet(ts_us: i64, total: u32) -> MvPacket {
        MvPacket {
            ts_us,
            frame_type: FrameType::I,
            total_mb: total,
            intra_count: total,
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

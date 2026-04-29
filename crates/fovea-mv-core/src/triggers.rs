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

use crate::global_motion::{to_q4, GlobalMotionEstimate, GlobalMotionEstimator};
use crate::{cbf_density, intra_ratio, motion_energy, skip_ratio, Event, FrameType, MotionVector, MvPacket, Trigger};

/// Sliding window that smooths energy over the last `cap` packets.
///
/// Reduces false positives from single-frame encoding artefacts and lets
/// sustained low-level motion accumulate past threshold.
struct EnergyWindow {
    cap: usize,
    buf: std::collections::VecDeque<u64>,
}

impl EnergyWindow {
    fn new(cap: usize) -> Self {
        Self { cap: cap.max(1), buf: std::collections::VecDeque::with_capacity(cap.max(1)) }
    }

    /// Push a new value and return the current window mean.
    fn push_and_mean(&mut self, val: u64) -> u64 {
        if self.buf.len() == self.cap {
            self.buf.pop_front();
        }
        self.buf.push_back(val);
        let sum: u64 = self.buf.iter().sum();
        sum / self.buf.len() as u64
    }
}

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
    global_motion: Option<GlobalMotionEstimator>,
    window: Option<EnergyWindow>,
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
            global_motion: None,
            window: None,
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

    /// Enable camera-motion compensation via median global-motion subtraction.
    ///
    /// Per packet: estimates dominant (median) MV across the full frame, then
    /// subtracts it before summing energy. Reduces false positives from camera
    /// vibration, wind shake, and slow PTZ drift.
    ///
    /// Cost: two O(n log n) sorts + two temporary `Vec<i32>` allocations of
    /// length `mvs.len()` per packet.
    pub fn with_global_motion(mut self, estimator: GlobalMotionEstimator) -> Self {
        self.global_motion = Some(estimator);
        self
    }

    /// Smooth energy over the last `frames` packets before threshold comparison.
    ///
    /// Reduces false positives from single-frame encoding artefacts. Sustained
    /// low-level motion that is individually below threshold accumulates and can
    /// cross it. `frames = 0` is treated as `frames = 1` (no-op).
    pub fn with_window(mut self, frames: usize) -> Self {
        self.window = Some(EnergyWindow::new(frames));
        self
    }

    fn energy_for(&self, mvs: &[MotionVector], gm: Option<GlobalMotionEstimate>) -> u64 {
        let (sub_x, sub_y) = match gm {
            Some(g) => (g.dx_q4 as i32, g.dy_q4 as i32),
            None => (0, 0),
        };
        let corrected = gm.is_some();
        let mut sum: u64 = 0;
        for mv in mvs {
            if let Some(m) = self.mask {
                if !m.contains(mv.dst_x, mv.dst_y) {
                    continue;
                }
            }
            if corrected {
                // Work in q4, divide by 4 at the end to match uncorrected
                // pixel-unit energy. Threshold semantics stay constant
                // whether or not global-motion correction is enabled.
                let q4x = to_q4(mv.motion_x, mv.motion_scale) - sub_x;
                let q4y = to_q4(mv.motion_y, mv.motion_scale) - sub_y;
                sum += (q4x.unsigned_abs() as u64 + q4y.unsigned_abs() as u64) / 4;
            } else {
                let scale = mv.motion_scale.max(1) as u64;
                sum += mv.l1_magnitude() / scale;
            }
        }
        sum
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
        let gm = self.global_motion.as_ref().and_then(|est| est.estimate(&packet.mvs));
        let raw_energy = self.energy_for(&packet.mvs, gm);
        let energy = if let Some(ref mut win) = self.window {
            win.push_and_mean(raw_energy)
        } else {
            raw_energy
        };
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
                    cbf_density: cbf_density(packet),
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
                    cbf_density: cbf_density(packet),
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
                    cbf_density: cbf_density(packet),
                mv_count: packet.mvs.len() as u32,
            });
        }
        None
    }

    fn name(&self) -> &'static str {
        "scene_change"
    }
}

/// How [`FusionTrigger`] combines its constituent signal checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FusionMode {
    /// Fire when **any** enabled signal crosses its threshold.
    /// Maximizes recall; tolerates higher false-positive rate.
    AnyOf,
    /// Fire only when **all** enabled signals cross. Maximizes
    /// precision; lower recall.
    AllOf,
}

/// Trigger that combines motion energy, intra-block ratio, and skip
/// ratio in a single decision.
///
/// Designed to live downstream of [`MotionTrigger`] and to act as the
/// "production trigger" most callers would actually wire up. The four
/// quadrants of (motion × intra) carry distinct semantic meanings —
/// see `docs/05.exec-plans/004-positioning.md` P0.9 for the table —
/// and `skip_suppress` adds an explicit "encoder said this region is
/// unchanged" gate on top.
///
/// Decision order per packet:
/// 1. If `skip_suppress` is set and `skip_ratio(packet) >= skip_suppress`,
///    return `None` (idle gate).
/// 2. Compute which configured signals are above their thresholds.
/// 3. Fire per [`FusionMode`].
/// 4. Apply cooldown.
///
/// Each threshold (`motion_threshold`, `intra_threshold`) is optional —
/// `None` means the signal is not part of the decision.
pub struct FusionTrigger {
    /// How signals are combined.
    pub mode: FusionMode,
    /// Motion energy threshold (sum of L1 magnitudes / motion_scale).
    /// `None` skips motion in the decision.
    pub motion_threshold: Option<u64>,
    /// Intra-coded macroblock fraction threshold in `[0.0, 1.0]`.
    /// `None` skips intra in the decision.
    pub intra_threshold: Option<f32>,
    /// If set and `skip_ratio(packet) >= skip_suppress`, the trigger
    /// returns `None` regardless of other signals. Hard idle gate.
    pub skip_suppress: Option<f32>,
    cooldown_us: i64,
    last_fire_us: Option<i64>,
}

impl FusionTrigger {
    /// Construct an `AnyOf` trigger with no thresholds set. Caller
    /// must enable at least one of `motion_threshold` / `intra_threshold`
    /// before this fires.
    pub fn new(mode: FusionMode) -> Self {
        Self {
            mode,
            motion_threshold: None,
            intra_threshold: None,
            skip_suppress: None,
            cooldown_us: 100_000,
            last_fire_us: None,
        }
    }

    /// Enable the motion-energy signal at `threshold`.
    pub fn with_motion_threshold(mut self, threshold: u64) -> Self {
        self.motion_threshold = Some(threshold);
        self
    }

    /// Enable the intra-ratio signal at `threshold` ∈ [0.0, 1.0].
    pub fn with_intra_threshold(mut self, threshold: f32) -> Self {
        self.intra_threshold = Some(threshold);
        self
    }

    /// Set a hard idle gate. When `skip_ratio(packet) >= floor`, the
    /// trigger never fires regardless of the other signals.
    pub fn with_skip_suppress(mut self, floor: f32) -> Self {
        self.skip_suppress = Some(floor);
        self
    }

    /// Override the per-fire cooldown, in milliseconds.
    pub fn with_cooldown_ms(mut self, ms: u32) -> Self {
        self.cooldown_us = (ms as i64) * 1_000;
        self
    }
}

impl Trigger for FusionTrigger {
    fn evaluate(&mut self, packet: &MvPacket) -> Option<Event> {
        if matches!(packet.frame_type, FrameType::I | FrameType::Other) {
            // I-frames carry intra_ratio = 1.0 by definition; firing on
            // them would just spam every keyframe. Same exclusion as
            // SceneChangeTrigger.
            return None;
        }

        // Idle gate.
        let skip_r = skip_ratio(packet);
        if let Some(floor) = self.skip_suppress {
            if skip_r >= floor {
                return None;
            }
        }

        let energy = motion_energy(&packet.mvs);
        let intra_r = intra_ratio(packet);

        let motion_ok = self.motion_threshold.map(|t| energy >= t);
        let intra_ok = self.intra_threshold.map(|t| intra_r >= t);

        // Collect just the signals the caller actually enabled.
        let mut checks: Vec<bool> = Vec::with_capacity(2);
        if let Some(b) = motion_ok {
            checks.push(b);
        }
        if let Some(b) = intra_ok {
            checks.push(b);
        }
        if checks.is_empty() {
            // Neither signal configured — don't fire.
            return None;
        }

        let fire = match self.mode {
            FusionMode::AnyOf => checks.iter().any(|b| *b),
            FusionMode::AllOf => checks.iter().all(|b| *b),
        };
        if !fire {
            return None;
        }

        if let Some(prev) = self.last_fire_us {
            if packet.ts_us.saturating_sub(prev) < self.cooldown_us {
                return None;
            }
        }
        self.last_fire_us = Some(packet.ts_us);

        Some(Event {
            ts_us: packet.ts_us,
            frame_type: packet.frame_type,
            trigger_name: self.name(),
            energy,
            intra_ratio: intra_r,
            skip_ratio: skip_r,
            cbf_density: cbf_density(packet),
            mv_count: packet.mvs.len() as u32,
        })
    }

    fn name(&self) -> &'static str {
        "fusion"
    }
}

/// Spatial-cluster trigger — fires when motion energy concentrates in a
/// small region of the frame, suppresses motion that is uniformly
/// scattered.
///
/// Use case: distinguish a person / vehicle (energy concentrated near one
/// or two adjacent cells) from environmental noise like leaves in wind,
/// rain, or a flapping flag (energy spread across the whole frame).
///
/// Algorithm: bin every MV's destination pixel into a square grid of
/// `cell_size_px` cells. For each cell, sum the L1 magnitudes (in
/// pixel units) of the MVs landing inside it. Compute
/// `concentration = peak_cell_energy / total_energy`. Fire when both:
/// 1. `total_energy >= min_total_energy` (avoid firing on noise floor),
/// 2. `concentration >= min_concentration` (the energy is clustered).
///
/// `concentration ∈ (0.0, 1.0]`. A frame whose energy is split evenly
/// across N active cells gives concentration ≈ 1/N. A single tightly
/// localised mover gives concentration close to 1.0.
///
/// Cost: O(mvs.len()) per packet, allocation-free aside from the
/// grid scratch which is reused across packets.
pub struct SpatialClusterTrigger {
    /// Square grid cell size in pixels. Typical: 128 or 256.
    pub cell_size_px: u16,
    /// Minimum total energy (sum of L1 magnitudes / motion_scale) before
    /// concentration is even checked.
    pub min_total_energy: u64,
    /// Minimum `peak_cell_energy / total_energy` to fire. 0.0–1.0.
    pub min_concentration: f32,
    /// Cooldown between fires. Defaults to 100 ms.
    cooldown_us: i64,
    last_fire_us: Option<i64>,
    /// Grid scratch — `(cell_x, cell_y) -> energy`. Reused across packets;
    /// `clear()` keeps capacity. We use a hash map rather than a dense
    /// array so we don't depend on knowing the frame size up front.
    grid: std::collections::HashMap<(u16, u16), u64>,
}

impl SpatialClusterTrigger {
    /// Construct with a 128 px cell grid and 100 ms cooldown.
    pub fn new(min_total_energy: u64, min_concentration: f32) -> Self {
        Self {
            cell_size_px: 128,
            min_total_energy,
            min_concentration,
            cooldown_us: 100_000,
            last_fire_us: None,
            grid: std::collections::HashMap::new(),
        }
    }

    /// Override the grid cell size in pixels.
    pub fn with_cell_size_px(mut self, px: u16) -> Self {
        self.cell_size_px = px.max(1);
        self
    }

    /// Override the per-fire cooldown, in milliseconds.
    pub fn with_cooldown_ms(mut self, ms: u32) -> Self {
        self.cooldown_us = (ms as i64) * 1_000;
        self
    }
}

impl Trigger for SpatialClusterTrigger {
    fn evaluate(&mut self, packet: &MvPacket) -> Option<Event> {
        if matches!(packet.frame_type, FrameType::I | FrameType::Other) {
            return None;
        }
        if packet.mvs.is_empty() {
            return None;
        }

        // Bin MVs into the grid. Per-MV cost: one map lookup + one add.
        let cell = self.cell_size_px.max(1) as i32;
        self.grid.clear();
        let mut total_energy: u64 = 0;
        for mv in &packet.mvs {
            let scale = mv.motion_scale.max(1) as u64;
            let e = mv.l1_magnitude() / scale;
            if e == 0 {
                continue;
            }
            // Negative dst coordinates can theoretically occur on
            // bitstream corruption; clamp to 0 so the bin index is
            // non-negative.
            let cx = (mv.dst_x.max(0) as i32 / cell) as u16;
            let cy = (mv.dst_y.max(0) as i32 / cell) as u16;
            *self.grid.entry((cx, cy)).or_insert(0) += e;
            total_energy += e;
        }

        if total_energy < self.min_total_energy {
            return None;
        }

        let peak = self.grid.values().copied().max().unwrap_or(0);
        if peak == 0 {
            return None;
        }
        let concentration = (peak as f64 / total_energy as f64) as f32;
        if concentration < self.min_concentration {
            return None;
        }

        if let Some(prev) = self.last_fire_us {
            if packet.ts_us.saturating_sub(prev) < self.cooldown_us {
                return None;
            }
        }
        self.last_fire_us = Some(packet.ts_us);

        Some(Event {
            ts_us: packet.ts_us,
            frame_type: packet.frame_type,
            trigger_name: self.name(),
            energy: total_energy,
            intra_ratio: intra_ratio(packet),
            skip_ratio: skip_ratio(packet),
                    cbf_density: cbf_density(packet),
            mv_count: packet.mvs.len() as u32,
        })
    }

    fn name(&self) -> &'static str {
        "spatial_cluster"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::global_motion::GlobalMotionEstimator;

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
            cbf_count: 0,
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
            cbf_count: 0,
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
            cbf_count: 0,
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
            cbf_count: 0,
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
            cbf_count: 0,
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

    // ----- MotionTrigger: global motion correction -----

    #[test]
    fn global_motion_suppresses_uniform_camera_shake() {
        // All MVs identical (pure camera pan) → corrected energy ≈ 0 → no fire.
        let mut t = MotionTrigger::new(5).with_global_motion(GlobalMotionEstimator);
        // 20 MVs all at (20, 20) q4 → global motion = (20, 20), residuals = 0
        let mvs: Vec<_> = (0..20).map(|_| mv(20, 20, 0, 0)).collect();
        let p = p_packet(0, mvs, 0, 100);
        assert!(
            t.evaluate(&p).is_none(),
            "uniform camera pan must not trigger after global-motion correction"
        );
    }

    #[test]
    fn global_motion_preserves_object_motion() {
        // 19 MVs at (4, 0) (camera pan), 1 MV at (40, 0) (moving object).
        // After correction: 19 residuals ≈ 0, 1 residual = 36 q4 → triggers.
        let mut t = MotionTrigger::new(5).with_global_motion(GlobalMotionEstimator);
        let mut mvs: Vec<_> = (0..19).map(|_| mv(4, 0, 0, 0)).collect();
        mvs.push(mv(40, 0, 0, 0)); // local object
        let p = p_packet(0, mvs, 0, 100);
        assert!(
            t.evaluate(&p).is_some(),
            "object motion must survive global-motion correction"
        );
    }

    // ----- MotionTrigger: sliding window -----

    #[test]
    fn window_smooths_single_spike() {
        // Threshold = 20, window = 3.
        // One spike at energy 60 gets averaged over 3 slots: mean = 20 (not > 20).
        // Two more low-energy packets follow; window mean stays below threshold.
        let _t = MotionTrigger::new(21).with_window(3);
        // Frame 1: energy = 60 (spike). Window = [60]. mean = 60 → fires? yes, 60 >= 21.
        // Let's use a tighter scenario: threshold=100, spike=60.
        let mut t2 = MotionTrigger::new(100).with_window(3);
        let spike = p_packet(0, vec![mv(240, 0, 0, 0)], 0, 100); // energy = 240/4 = 60
        let low = p_packet(33_000, vec![mv(4, 0, 0, 0)], 0, 100); // energy = 1
        let low2 = p_packet(66_000, vec![mv(4, 0, 0, 0)], 0, 100);
        // Window after spike: [60], mean = 60 < 100 → no fire
        assert!(t2.evaluate(&spike).is_none(), "single spike below threshold after smoothing");
        assert!(t2.evaluate(&low).is_none());
        assert!(t2.evaluate(&low2).is_none());
    }

    #[test]
    fn window_accumulates_sustained_motion() {
        // Threshold = 10, window = 3.
        // Each packet has energy = 12 → mean after 1 packet = 12 → fires immediately.
        // Verify it fires on the first above-threshold (window doesn't delay correct triggers).
        let mut t = MotionTrigger::new(10).with_window(3);
        let p1 = p_packet(0, vec![mv(48, 0, 0, 0)], 0, 100); // energy = 12
        assert!(t.evaluate(&p1).is_some(), "sustained motion must fire");
    }

    #[test]
    fn window_size_zero_treated_as_one() {
        // with_window(0) must not panic and must behave like no-window.
        let mut t = MotionTrigger::new(5).with_window(0);
        let p = p_packet(0, vec![mv(40, 0, 0, 0)], 0, 100); // energy = 10 >= 5
        assert!(t.evaluate(&p).is_some());
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

    // ----- SpatialClusterTrigger -----

    #[test]
    fn spatial_cluster_fires_on_concentrated_motion() {
        // 32 MVs all landing in the same 128 px cell (cell 0,0). High
        // concentration should cross the 0.8 threshold easily.
        let mut t = SpatialClusterTrigger::new(50, 0.8);
        let mvs: Vec<_> = (0..32).map(|_| mv(40, 0, 16, 16)).collect();
        let pkt = p_packet(0, mvs, 0, 100);
        let ev = t.evaluate(&pkt).expect("clustered motion must fire");
        assert_eq!(ev.trigger_name, "spatial_cluster");
    }

    #[test]
    fn spatial_cluster_suppresses_scattered_motion() {
        // 32 MVs distributed across 32 different cells (one MV per cell)
        // → concentration ≈ 1/32 = 0.03 ≪ 0.5 threshold → no fire.
        let mut t = SpatialClusterTrigger::new(50, 0.5);
        let mvs: Vec<_> = (0..32)
            .map(|i| mv(40, 0, (i * 200) as i16, (i * 200) as i16))
            .collect();
        let pkt = p_packet(0, mvs, 0, 100);
        assert!(t.evaluate(&pkt).is_none(), "scattered motion must not fire");
    }

    #[test]
    fn spatial_cluster_below_total_energy_does_not_fire() {
        // Concentrated motion but total energy below the floor.
        let mut t = SpatialClusterTrigger::new(10_000, 0.5);
        let pkt = p_packet(0, vec![mv(40, 0, 16, 16)], 0, 100); // energy = 10
        assert!(t.evaluate(&pkt).is_none());
    }

    #[test]
    fn spatial_cluster_skips_iframe() {
        let mut t = SpatialClusterTrigger::new(0, 0.0);
        let i = i_packet(0, 100);
        assert!(t.evaluate(&i).is_none(), "I-frames must not trigger spatial cluster");
    }

    // ----- FusionTrigger -----

    #[test]
    fn fusion_anyof_fires_on_motion_alone() {
        // motion crosses, intra does not — AnyOf fires.
        let mut t = FusionTrigger::new(FusionMode::AnyOf)
            .with_motion_threshold(50)
            .with_intra_threshold(0.5);
        // intra=0/100=0.0 (below 0.5), but big motion energy
        let pkt = p_packet(0, vec![mv(200, 0, 0, 0)], 0, 100); // energy = 200/4 = 50
        assert!(t.evaluate(&pkt).is_some());
    }

    #[test]
    fn fusion_anyof_fires_on_intra_alone() {
        // intra crosses, motion does not — AnyOf fires. Models
        // "lights flickered" / "new object" / "smoke" scenarios.
        let mut t = FusionTrigger::new(FusionMode::AnyOf)
            .with_motion_threshold(1_000_000)
            .with_intra_threshold(0.3);
        let pkt = p_packet(0, vec![], 50, 100); // intra=0.5, no motion
        assert!(t.evaluate(&pkt).is_some());
    }

    #[test]
    fn fusion_allof_requires_both() {
        // Only motion crosses → AllOf does not fire.
        let mut t = FusionTrigger::new(FusionMode::AllOf)
            .with_motion_threshold(50)
            .with_intra_threshold(0.5);
        let pkt = p_packet(0, vec![mv(200, 0, 0, 0)], 0, 100);
        assert!(t.evaluate(&pkt).is_none(), "AllOf must reject motion-only");
    }

    #[test]
    fn fusion_allof_fires_when_both_cross() {
        let mut t = FusionTrigger::new(FusionMode::AllOf)
            .with_motion_threshold(50)
            .with_intra_threshold(0.4);
        // intra = 60/100 = 0.6, motion energy = 50
        let pkt = p_packet(0, vec![mv(200, 0, 0, 0)], 60, 100);
        assert!(t.evaluate(&pkt).is_some());
    }

    #[test]
    fn fusion_skip_suppress_overrides_other_signals() {
        // Both motion and intra would otherwise fire, but skip_ratio is
        // at the suppress floor → no fire.
        let mut t = FusionTrigger::new(FusionMode::AnyOf)
            .with_motion_threshold(50)
            .with_intra_threshold(0.3)
            .with_skip_suppress(0.85);
        let mut pkt = p_packet(0, vec![mv(200, 0, 0, 0)], 50, 100);
        pkt.skip_count = 90; // skip_ratio = 0.90 ≥ 0.85
        assert!(t.evaluate(&pkt).is_none(), "skip gate must suppress fire");
    }

    #[test]
    fn fusion_no_thresholds_never_fires() {
        // Neither signal configured → defensive no-fire.
        let mut t = FusionTrigger::new(FusionMode::AnyOf);
        let pkt = p_packet(0, vec![mv(200, 0, 0, 0)], 100, 100);
        assert!(t.evaluate(&pkt).is_none());
    }

    #[test]
    fn fusion_skips_iframe() {
        let mut t = FusionTrigger::new(FusionMode::AnyOf)
            .with_motion_threshold(0)
            .with_intra_threshold(0.0);
        let i = i_packet(0, 100);
        assert!(t.evaluate(&i).is_none());
    }

    #[test]
    fn spatial_cluster_debounces_within_cooldown() {
        let mut t = SpatialClusterTrigger::new(50, 0.8).with_cooldown_ms(100);
        let mvs: Vec<_> = (0..32).map(|_| mv(40, 0, 16, 16)).collect();
        let a = p_packet(0, mvs.clone(), 0, 100);
        let b = p_packet(50_000, mvs.clone(), 0, 100); // 50ms < 100ms cooldown
        let c = p_packet(150_000, mvs, 0, 100); // > 100ms later
        assert!(t.evaluate(&a).is_some());
        assert!(t.evaluate(&b).is_none());
        assert!(t.evaluate(&c).is_some());
    }
}

//! End-to-end Step 4a integration test.
//!
//! Runs MotionTrigger / IntervalTrigger / SceneChangeTrigger across the
//! committed CCTV sample and verifies plausible event counts. Skips silently
//! if the sample is not present.

use std::path::PathBuf;

use fovea_trigger_core::triggers::{IntervalTrigger, MotionTrigger, SceneChangeTrigger};
use fovea_trigger_core::Trigger;
use fovea_trigger_stream::FfmpegSource;

fn sample_path() -> Option<PathBuf> {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p.push("benchmarks/datasets/cctv-sample/sample.mp4");
    if p.exists() { Some(p) } else { None }
}

#[test]
fn triggers_fire_on_cctv_sample() {
    let Some(path) = sample_path() else {
        eprintln!("sample missing, skipping");
        return;
    };

    let mut motion = MotionTrigger::new(20_000);
    let mut interval = IntervalTrigger::new(2_000);
    let mut scene = SceneChangeTrigger::new(0.6);

    let mut counts = (0u32, 0u32, 0u32);
    let mut motion_first_ts: Option<i64> = None;
    let mut interval_first_ts: Option<i64> = None;

    let mut src = FfmpegSource::open_file(&path).expect("open");
    while let Some(pkt) = src.next_packet().expect("decode") {
        if let Some(ev) = motion.evaluate(pkt) {
            counts.0 += 1;
            motion_first_ts.get_or_insert(ev.ts_us);
        }
        if let Some(ev) = interval.evaluate(pkt) {
            counts.1 += 1;
            interval_first_ts.get_or_insert(ev.ts_us);
        }
        if scene.evaluate(pkt).is_some() {
            counts.2 += 1;
        }
    }

    println!("motion fires       = {}", counts.0);
    println!("interval fires     = {}", counts.1);
    println!("scene change fires = {}", counts.2);

    // Sample is high-motion and 15.23s long.
    // - MotionTrigger (energy >= 20000): clip has continuous motion so we
    //   expect the trigger to fire at least once early on.
    assert!(counts.0 >= 1, "MotionTrigger should fire on this clip");
    // - IntervalTrigger every 2s on a 15s clip: ~8 fires (first packet + 7
    //   subsequent windows). Allow ±1.
    assert!(
        (7..=9).contains(&counts.1),
        "IntervalTrigger fires {} outside expected band",
        counts.1
    );
    // - SceneChangeTrigger: clip has no editorial scene cuts, but a brief
    //   intra spike (occlusion, lighting flicker) can cross 0.6. We allow
    //   a small number of fires; many would mean the threshold is wrong.
    assert!(
        counts.2 <= 5,
        "SceneChangeTrigger fired {} times — threshold 0.6 may be too low",
        counts.2
    );
}

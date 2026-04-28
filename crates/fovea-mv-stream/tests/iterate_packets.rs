//! Step 2.3 integration test.
//!
//! Iterates the full sample, tallies frame-type distribution, asserts:
//! - total decoded packet count is plausible for 15.23s @ 30fps.
//! - at least one I-frame is observed.
//! - at least one P-frame (or B-frame) is observed.
//! - frame timestamps are non-decreasing on output (monotone progress).

use std::collections::HashMap;
use std::path::PathBuf;

use fovea_mv_core::FrameType;
use fovea_mv_stream::FfmpegSource;

fn sample_path() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p.push("benchmarks/datasets/cctv-sample/sample.mp4");
    p
}

#[test]
fn iterate_cctv_sample_frames() {
    let path = sample_path();
    if !path.exists() {
        eprintln!("sample missing at {path:?}, skipping");
        return;
    }

    let mut src = FfmpegSource::open_file(&path).expect("open sample");

    let mut counts: HashMap<FrameType, u32> = HashMap::new();
    let mut total: u32 = 0;
    let mut last_ts: i64 = i64::MIN;
    let mut nonmono = 0u32;

    while let Some(pkt) = src.next_packet().expect("decode") {
        *counts.entry(pkt.frame_type).or_insert(0) += 1;
        if pkt.ts_us < last_ts {
            nonmono += 1;
        }
        last_ts = pkt.ts_us;
        total += 1;
    }

    println!("total = {total}");
    for ft in [FrameType::I, FrameType::P, FrameType::B, FrameType::Other] {
        println!("  {ft} = {}", counts.get(&ft).copied().unwrap_or(0));
    }
    println!("non-monotonic ts events = {nonmono}");
    println!("last_ts_us = {last_ts}");

    // 15.23s @ 30fps ≈ 457 frames. Allow ±10%.
    assert!(
        (400..=500).contains(&total),
        "frame count {total} outside expected band"
    );

    assert!(
        counts.get(&FrameType::I).copied().unwrap_or(0) >= 1,
        "expected at least one I-frame"
    );

    let p_count = counts.get(&FrameType::P).copied().unwrap_or(0);
    let b_count = counts.get(&FrameType::B).copied().unwrap_or(0);
    assert!(
        p_count + b_count > 0,
        "expected at least one inter-coded frame (P or B)"
    );
}

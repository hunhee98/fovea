//! Step 2.4 integration test.
//!
//! Iterates the full sample with `+export_mvs` enabled. Asserts:
//! - I-frames have zero motion vectors (intra-only coding).
//! - P/B-frames have non-zero MV count for at least 80% of inter packets.
//! - Aggregate `motion_energy` over the run is non-zero (the clip has motion).

use std::path::PathBuf;

use fovea_mv_core::{motion_energy, FrameType};
use fovea_mv_stream::FfmpegSource;

fn sample_path() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p.push("benchmarks/datasets/cctv-sample/sample.mp4");
    p
}

#[test]
fn extract_mvs_from_sample() {
    let path = sample_path();
    if !path.exists() {
        eprintln!("sample missing at {path:?}, skipping");
        return;
    }

    let mut src = FfmpegSource::open_file(&path).expect("open sample");

    let mut i_frames = 0u32;
    let mut inter_frames = 0u32;
    let mut inter_with_mvs = 0u32;
    let mut total_mv_count: u64 = 0;
    let mut total_energy: u64 = 0;
    let mut max_mvs_in_one_packet: u32 = 0;

    while let Some(pkt) = src.next_packet().expect("decode") {
        let mv_n = pkt.mvs.len() as u32;
        total_mv_count += mv_n as u64;
        total_energy += motion_energy(&pkt.mvs);
        if mv_n > max_mvs_in_one_packet {
            max_mvs_in_one_packet = mv_n;
        }
        match pkt.frame_type {
            FrameType::I => {
                i_frames += 1;
                assert_eq!(
                    mv_n, 0,
                    "I-frame at ts_us={} unexpectedly has {mv_n} MVs",
                    pkt.ts_us
                );
            }
            FrameType::P | FrameType::B => {
                inter_frames += 1;
                if mv_n > 0 {
                    inter_with_mvs += 1;
                }
            }
            FrameType::Other => {}
        }
    }

    println!("I-frames        = {i_frames}");
    println!("inter frames    = {inter_frames}");
    println!("inter w/ MVs    = {inter_with_mvs}");
    println!("total MV count  = {total_mv_count}");
    println!("total energy    = {total_energy}");
    println!("max MVs/packet  = {max_mvs_in_one_packet}");

    assert!(i_frames >= 1);
    assert!(inter_frames > 0);
    let coverage = inter_with_mvs as f32 / inter_frames as f32;
    println!("inter MV coverage = {coverage:.3}");
    assert!(
        coverage >= 0.80,
        "only {:.1}% of inter frames have MVs (want >= 80%)",
        coverage * 100.0
    );
    assert!(total_energy > 0, "expected non-zero motion energy");
}

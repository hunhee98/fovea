//! Step 2.5 integration test.
//!
//! Validates `total_mb` and the approximate `intra_count` derived from MV
//! coverage on the committed sample.
//!
//! Expectations:
//! - `total_mb == ceil(1080/16) * ceil(1920/16) = 68 * 120 = 8160`.
//! - I-frames: `intra_ratio == 1.0`.
//! - Inter-frame mean `intra_ratio` < 0.5 (sample has lots of motion).

use std::path::PathBuf;

use fovea_mv_core::{intra_ratio, FrameType};
use fovea_mv_stream::FfmpegSource;

fn sample_path() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p.push("benchmarks/datasets/cctv-sample/sample.mp4");
    p
}

#[test]
fn intra_count_consistency() {
    let path = sample_path();
    if !path.exists() {
        eprintln!("sample missing at {path:?}, skipping");
        return;
    }

    let mut src = FfmpegSource::open_file(&path).expect("open sample");

    // 1080x1920 portrait. mb_w = 68, mb_h = 120, total = 8160.
    let expected_total_mb = 68u32 * 120;

    let mut inter_ratio_sum = 0.0f32;
    let mut inter_count = 0u32;
    let mut i_seen = 0u32;

    while let Some(pkt) = src.next_packet().expect("decode") {
        assert_eq!(
            pkt.total_mb, expected_total_mb,
            "total_mb mismatch at ts_us={}",
            pkt.ts_us
        );
        assert!(pkt.intra_count <= pkt.total_mb);

        let r = intra_ratio(pkt);
        match pkt.frame_type {
            FrameType::I => {
                i_seen += 1;
                assert!(
                    (r - 1.0).abs() < 1e-6,
                    "I-frame intra_ratio {r} != 1.0"
                );
            }
            FrameType::P | FrameType::B => {
                inter_ratio_sum += r;
                inter_count += 1;
            }
            FrameType::Other => {}
        }
    }

    assert!(i_seen >= 1);
    assert!(inter_count > 0);
    let mean_inter_intra_ratio = inter_ratio_sum / inter_count as f32;
    println!("mean inter-frame intra_ratio = {mean_inter_intra_ratio:.4}");
    println!("inter frames = {inter_count}, I-frames = {i_seen}");
    assert!(
        mean_inter_intra_ratio < 0.5,
        "mean inter-frame intra_ratio {mean_inter_intra_ratio} too high (sample has continuous motion, expected lots of inter coverage)"
    );
}

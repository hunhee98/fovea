//! Regression tests for the HEVC fixes shipped on
//! `feat/mv-step2-h264-extraction` after 002. Each fix below wedged a
//! production HEVC source into the pipeline; the corresponding test
//! pins the expected shape so a future refactor doesn't silently
//! break it again.
//!
//! - `hevc_pts_passthrough_monotonic` — `de265_push_data` now carries
//!   the per-packet PTS; before that fix every HEVC frame's `ts_us`
//!   came back as 0 and `IntervalTrigger` only fired once.
//! - `hevc_event_decode_returns_rgb` — `last_frame_rgb` used to
//!   error out for HEVC sources; the libde265 plane copy + BT.601
//!   YUV→RGB path now returns a real ndarray.
//! - `hevc_cbf_density_in_range` — the T2 CBF accessor lives on
//!   `tu_info.TU_FLAG_NONZERO_COEFF`; this test confirms
//!   `cbf_density` lands in [0.0, 1.0] and that the per-frame
//!   distribution matches the broad shape we expect on a normal
//!   inter-coded clip.
//!
//! All three load the committed `cctv-sample-hevc/sample.mp4`. They
//! skip silently when the fixture is missing so contributors who
//! haven't run the dataset download script still see green.

use std::path::PathBuf;

use fovea_mv_core::triggers::{IntervalTrigger, MotionTrigger};
use fovea_mv_core::{cbf_density, Trigger};
use fovea_mv_stream::FfmpegSource;

fn hevc_sample_path() -> Option<PathBuf> {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p.push("benchmarks/datasets/cctv-sample-hevc/sample.mp4");
    if p.exists() {
        Some(p)
    } else {
        None
    }
}

#[test]
fn hevc_pts_passthrough_monotonic() {
    let Some(path) = hevc_sample_path() else {
        eprintln!("HEVC sample missing, skipping");
        return;
    };

    let mut src = FfmpegSource::open_file(&path).expect("open");
    let mut interval = IntervalTrigger::new(500); // fire every 0.5 s of stream time
    let mut fire_timestamps: Vec<i64> = Vec::new();

    while fire_timestamps.len() < 4 {
        let Some(pkt) = src.next_packet().expect("decode") else {
            break;
        };
        if let Some(ev) = interval.evaluate(pkt) {
            fire_timestamps.push(ev.ts_us);
        }
    }

    assert!(
        fire_timestamps.len() >= 3,
        "IntervalTrigger should have fired at least 3 times across the clip; got {}",
        fire_timestamps.len()
    );
    // Strictly monotonic — guards against the regression where
    // libde265 returned 0 for every frame and IntervalTrigger
    // collapsed to a single fire.
    for window in fire_timestamps.windows(2) {
        assert!(
            window[1] > window[0],
            "IntervalTrigger PTS must be monotonic; got {} then {}",
            window[0],
            window[1]
        );
    }
    // First fire should be at or near 0 us, second roughly +500 ms.
    // Wide tolerance (±300 ms) accommodates GOP-aligned PTS jitter.
    let delta_us = fire_timestamps[1] - fire_timestamps[0];
    assert!(
        (200_000..=800_000).contains(&delta_us),
        "expected ~500ms between first two interval fires, got {delta_us} us"
    );
}

#[test]
fn hevc_event_decode_returns_rgb() {
    let Some(path) = hevc_sample_path() else {
        eprintln!("HEVC sample missing, skipping");
        return;
    };

    let mut src = FfmpegSource::open_file(&path).expect("open");
    // Per-MB threshold low enough that the very first inter-coded
    // packet trips the trigger.
    let mut motion = MotionTrigger::with_per_mb_threshold(0.1);

    let mut decoded_once = false;
    while !decoded_once {
        let Some(pkt) = src.next_packet().expect("decode") else {
            break;
        };
        if motion.evaluate(pkt).is_some() {
            let rgb = src.last_frame_rgb().expect("HEVC last_frame_rgb must succeed");
            let info = src.info().clone();
            let expected_len = (info.width as usize) * (info.height as usize) * 3;
            assert_eq!(
                rgb.len(),
                expected_len,
                "RGB buffer should be width*height*3 bytes; got {} expected {}",
                rgb.len(),
                expected_len
            );
            // Defensive sanity: output is not the all-zero / all-255
            // buffer that a broken plane copy would produce.
            let mean = rgb.iter().map(|&b| b as u64).sum::<u64>() / (rgb.len() as u64);
            assert!(
                (8..=247).contains(&mean),
                "decoded RGB mean luminance suspiciously extreme: {mean}"
            );
            decoded_once = true;
        }
    }
    assert!(
        decoded_once,
        "MotionTrigger never fired on the HEVC sample; cannot test decode()"
    );
}

#[test]
fn hevc_cbf_density_in_range() {
    let Some(path) = hevc_sample_path() else {
        eprintln!("HEVC sample missing, skipping");
        return;
    };

    let mut src = FfmpegSource::open_file(&path).expect("open");
    let mut samples: Vec<f32> = Vec::new();

    // First ~120 frames is plenty to cover one full GOP.
    for _ in 0..120 {
        let Some(pkt) = src.next_packet().expect("decode") else {
            break;
        };
        let d = cbf_density(pkt);
        assert!(
            (0.0..=1.0).contains(&d),
            "cbf_density out of [0.0, 1.0]: {d}"
        );
        samples.push(d);
    }
    assert!(
        samples.len() >= 30,
        "expected at least 30 packets in the HEVC sample, got {}",
        samples.len()
    );
    // At least one frame should have a nonzero CBF coverage on a
    // normal inter-coded clip (otherwise the accessor is silently
    // returning 0 everywhere — exactly the regression we want to catch).
    assert!(
        samples.iter().any(|&d| d > 0.0),
        "every cbf_density was 0.0 — accessor likely returning empty stats"
    );
}

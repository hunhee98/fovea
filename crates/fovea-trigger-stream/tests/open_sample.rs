//! Step 2.2 integration test.
//!
//! Opens the committed CCTV sample and asserts captured metadata matches
//! `ffprobe` output. Skips silently if the sample file is absent so a clean
//! checkout without datasets still passes.

use std::path::PathBuf;

use fovea_trigger_stream::FfmpegSource;

fn sample_path() -> PathBuf {
    // CARGO_MANIFEST_DIR = .../crates/fovea-trigger-stream
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // crates
    p.pop(); // repo root
    p.push("benchmarks/datasets/cctv-sample/sample.mp4");
    p
}

#[test]
fn open_cctv_sample_metadata() {
    let path = sample_path();
    if !path.exists() {
        eprintln!("sample missing at {path:?}, skipping");
        return;
    }

    let src = FfmpegSource::open_file(&path).expect("open sample");
    let info = src.info();

    println!("codec        = {:?}", info.codec);
    println!("resolution   = {}x{}", info.width, info.height);
    println!("frame_rate   = {}/{}", info.frame_rate.0, info.frame_rate.1);
    println!("time_base    = {}/{}", info.time_base.0, info.time_base.1);
    println!("duration_us  = {:?}", info.duration_us);
    println!("nb_frames    = {:?}", info.nb_frames_hint);

    assert_eq!(info.codec, ffmpeg_next::codec::Id::H264);
    assert_eq!(info.width, 1080);
    assert_eq!(info.height, 1920);
    assert_eq!(info.frame_rate, (30, 1));

    let dur = info.duration_us.expect("duration known");
    // 15.23s ± 0.5s (container reports duration in AV_TIME_BASE = 1e6).
    assert!(
        (14_500_000..=15_500_000).contains(&dur),
        "duration {dur} outside expected range"
    );
}

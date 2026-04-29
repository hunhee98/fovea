//! Verify FfmpegSource transparently handles raw H.264 elementary streams
//! (`.h264` files without a container).
//!
//! The fixture is regenerated on demand from the committed sample.mp4 via
//! ffmpeg bitstream copy, so the test is hermetic when the sample is
//! present and skips otherwise.

use std::path::PathBuf;
use std::process::Command;

use fovea_trigger_core::FrameType;
use fovea_trigger_stream::FfmpegSource;

fn sample_path() -> Option<PathBuf> {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p.push("benchmarks/datasets/cctv-sample/sample.mp4");
    if p.exists() { Some(p) } else { None }
}

fn ensure_raw_h264(mp4: &std::path::Path, out: &std::path::Path) -> bool {
    if out.exists() {
        return true;
    }
    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-loglevel",
            "error",
            "-i",
            mp4.to_str().unwrap(),
            "-c:v",
            "copy",
            "-bsf:v",
            "h264_mp4toannexb",
            "-an",
            out.to_str().unwrap(),
        ])
        .status();
    matches!(status, Ok(s) if s.success())
}

#[test]
fn open_raw_h264_elementary_stream() {
    let Some(mp4) = sample_path() else {
        eprintln!("sample missing, skipping");
        return;
    };
    let raw = std::env::temp_dir().join("fovea-step3-raw-sample.h264");
    if !ensure_raw_h264(&mp4, &raw) {
        eprintln!("ffmpeg not available, skipping");
        return;
    }

    let mut src = FfmpegSource::open_file(&raw).expect("open raw h264");
    let info = src.info().clone();
    assert_eq!(info.width, 1080);
    assert_eq!(info.height, 1920);

    let mut total = 0u32;
    let mut i = 0u32;
    let mut inter = 0u32;
    while let Some(pkt) = src.next_packet().expect("decode") {
        total += 1;
        match pkt.frame_type {
            FrameType::I => i += 1,
            FrameType::P | FrameType::B => inter += 1,
            FrameType::Other => {}
        }
    }
    println!("raw h264 — total={total} I={i} inter={inter}");
    assert!((400..=500).contains(&total), "raw frame count {total}");
    assert!(i >= 1);
    assert!(inter > 0);
}

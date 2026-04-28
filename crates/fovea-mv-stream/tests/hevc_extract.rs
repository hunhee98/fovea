//! HEVC support is currently blocked at the FFmpeg upstream layer:
//! libavcodec's HEVC decoder does not emit `AV_FRAME_DATA_MOTION_VECTORS`
//! even with `AV_CODEC_FLAG2_EXPORT_MVS` set, while the H.264 decoder
//! does. This test pins the resulting policy: an HEVC source is rejected
//! at open time with [`SourceError::UnsupportedCodec`] so callers don't
//! get a silent "trigger never fires" failure mode.
//!
//! See `docs/05.exec-plans/001-mvtrigger-mvp.md` for the production-mode
//! follow-up that revisits this when (a) FFmpeg upstream gains HEVC MV
//! export, or (b) we add a custom HEVC MV parser.

use std::path::PathBuf;

use fovea_mv_stream::{FfmpegSource, SourceError};

fn hevc_path() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p.push("benchmarks/datasets/cctv-sample-hevc/sample.mp4");
    p
}

#[test]
fn hevc_source_is_rejected_with_clear_error() {
    let path = hevc_path();
    if !path.exists() {
        eprintln!("hevc fixture missing at {path:?}, skipping");
        return;
    }

    let result = FfmpegSource::open_file(&path);
    match result {
        Err(SourceError::UnsupportedCodec(id)) => {
            assert_eq!(id, ffmpeg_next::codec::Id::HEVC);
            println!("HEVC rejected as expected: {id:?}");
        }
        Err(other) => panic!("expected UnsupportedCodec, got {other:?}"),
        Ok(_) => panic!("expected open to fail on HEVC source"),
    }
}

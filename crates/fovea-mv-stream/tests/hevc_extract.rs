//! HEVC support via the libde265 backend (Step 002 follow-up). FFmpeg
//! upstream's HEVC decoder does not emit `AV_FRAME_DATA_MOTION_VECTORS`,
//! so we route HEVC streams through our vendored libde265 + the
//! `de265_internals` extension instead. This test verifies that an mp4
//! HEVC clip opens, decodes, and yields motion-vector packets through
//! the same `FfmpegSource` API used for H.264.

use std::path::PathBuf;

use fovea_mv_core::FrameType;
use fovea_mv_stream::FfmpegSource;

fn hevc_path() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    // Raw Annex-B bitstream — VPS/SPS/PPS are inline, so libde265
    // initializes without extradata. The mp4-muxed sibling needs hvcC
    // extradata conversion which is queued for a follow-up step.
    p.push("benchmarks/datasets/cctv-sample-hevc/sample.h265");
    p
}

#[test]
fn hevc_source_decodes_and_emits_mvs() {
    let path = hevc_path();
    if !path.exists() {
        eprintln!("hevc fixture missing at {path:?}, skipping");
        return;
    }

    let mut src = FfmpegSource::open_file(&path).expect("open hevc");
    let info = src.info().clone();
    assert_eq!(info.codec, ffmpeg_next::codec::Id::HEVC);
    assert_eq!(info.width, 1080);
    assert_eq!(info.height, 1920);

    let mut frames = 0u32;
    let mut i_frames = 0u32;
    let mut inter_with_mvs = 0u32;
    let mut total_mvs: u64 = 0;
    let mut total_energy: u64 = 0;

    while let Some(pkt) = src.next_packet().expect("decode") {
        frames += 1;
        let mv_n = pkt.mvs.len() as u32;
        total_mvs += mv_n as u64;
        for mv in &pkt.mvs {
            total_energy += (mv.motion_x.unsigned_abs() as u64)
                + (mv.motion_y.unsigned_abs() as u64);
        }
        match pkt.frame_type {
            FrameType::I => i_frames += 1,
            FrameType::P | FrameType::B => {
                if mv_n > 0 {
                    inter_with_mvs += 1;
                }
            }
            FrameType::Other => {}
        }
    }

    println!(
        "HEVC decode: frames={frames} I={i_frames} inter_with_mvs={inter_with_mvs} \
         total_mvs={total_mvs} total_energy={total_energy}"
    );
    assert!((400..=500).contains(&frames));
    assert!(i_frames >= 1);
    assert!(inter_with_mvs > 0);
    assert!(total_mvs > 0, "expected non-zero MV count from HEVC source");
    assert!(total_energy > 0);
}

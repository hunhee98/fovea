//! Decode the HEVC fixture via our libde265 binding and verify per-frame
//! MV statistics match what the standalone C smoke test produced.

use std::path::PathBuf;

use fovea_mv_stream::hevc::{HevcDecoder, PbInfo};

fn raw_h265_path() -> PathBuf {
    // The committed HEVC fixture is mp4-muxed; tests assume a sibling
    // raw-bitstream version produced by:
    //   ffmpeg -i sample.mp4 -c:v copy -bsf:v hevc_mp4toannexb -an sample.h265
    //
    // Falls back to /tmp if the per-clip fixture is absent.
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p.push("benchmarks/datasets/cctv-sample-hevc/sample.h265");
    if p.exists() {
        return p;
    }
    PathBuf::from("/tmp/hevc-research/sample.h265")
}

fn fill_one_frame(dec: &mut HevcDecoder, eof_signaled: &mut bool, src: &[u8], cursor: &mut usize) -> Option<FrameStats> {
    // Loop the canonical libde265 decode/display state machine until we
    // either get a frame or run out of work.
    loop {
        // 1. If we have input bytes, push a chunk before stepping.
        if *cursor < src.len() {
            let end = (*cursor + 64 * 1024).min(src.len());
            dec.push(&src[*cursor..end], 0).expect("push");
            *cursor = end;
            if *cursor >= src.len() && !*eof_signaled {
                dec.flush().expect("flush");
                *eof_signaled = true;
            }
        } else if !*eof_signaled {
            dec.flush().expect("flush");
            *eof_signaled = true;
        }

        let (more, frame) = dec.decode_step().expect("decode_step");
        if let Some(frame) = frame {
            let layout = frame.pb_layout();
            let info = frame.pb_info();
            let mut with_motion = 0u32;
            let mut energy: i64 = 0;
            for c in &info {
                if c.has_motion() {
                    with_motion += 1;
                }
                energy += c.mv0_x.abs() as i64 + c.mv0_y.abs() as i64;
            }
            return Some(FrameStats {
                width: frame.width(),
                height: frame.height(),
                pb_w: layout.0,
                pb_h: layout.1,
                with_motion,
                energy,
                cell_count: info.len() as u32,
            });
        }
        if !more && *cursor >= src.len() {
            return None;
        }
    }
}

#[derive(Debug)]
struct FrameStats {
    width: u32,
    height: u32,
    pb_w: u32,
    pb_h: u32,
    with_motion: u32,
    energy: i64,
    cell_count: u32,
}

fn _unused(p: &PbInfo) -> i32 { p.ref_poc0 as i32 }

#[test]
fn libde265_binding_decodes_hevc_clip() {
    let path = raw_h265_path();
    if !path.exists() {
        eprintln!("HEVC raw bitstream not found at {path:?}; skipping");
        return;
    }
    let bytes = std::fs::read(&path).expect("read raw h265");

    let mut dec = HevcDecoder::new().expect("decoder");
    let mut eof = false;
    let mut cursor = 0usize;

    let mut frames: Vec<FrameStats> = Vec::new();
    while let Some(stats) = fill_one_frame(&mut dec, &mut eof, &bytes, &mut cursor) {
        frames.push(stats);
    }

    println!("decoded {} frames", frames.len());
    assert!(
        frames.len() >= 400,
        "expected ~457 frames, got {}",
        frames.len()
    );
    let f0 = &frames[0];
    println!("frame 0: {}x{} pb {}x{} with_motion={} energy={}",
             f0.width, f0.height, f0.pb_w, f0.pb_h, f0.with_motion, f0.energy);
    assert_eq!(f0.width, 1080);
    assert_eq!(f0.height, 1920);
    assert_eq!(f0.pb_w, 270);
    assert_eq!(f0.pb_h, 480);
    assert_eq!(f0.cell_count, 270 * 480);
    // I-frame: zero MVs.
    assert_eq!(f0.with_motion, 0, "I-frame must have 0 motion entries");
    assert_eq!(f0.energy, 0);

    // Subsequent frames are inter-coded: most cells should carry motion
    // and energy should be substantial.
    let mut inter_with_motion = 0u32;
    let mut total_energy: i64 = 0;
    for f in &frames[1..] {
        if f.with_motion > 0 {
            inter_with_motion += 1;
        }
        total_energy += f.energy;
    }
    let inter_total = (frames.len() - 1) as u32;
    println!("inter frames with motion: {}/{}", inter_with_motion, inter_total);
    println!("aggregate energy (all inter): {}", total_energy);
    assert!(
        inter_with_motion as f32 / inter_total as f32 >= 0.95,
        "{}/{} inter frames lacked motion",
        inter_total - inter_with_motion,
        inter_total
    );
    assert!(total_energy > 1_000_000, "aggregate energy unexpectedly low");
}

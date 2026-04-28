//! Sanity probe: compare the existing PB-derived intra-ratio heuristic
//! against the new CB-stats accessor on every frame of a raw .h265 clip.
//!
//! What this answers: are the two numbers numerically close, or does
//! reading PredMode directly from `cb_info` reveal a meaningfully
//! different intra fraction than "no reference frame ⇒ intra"?
//!
//! Run:
//!   cargo run -p fovea-mv-stream --example cb_stats_compare -- <path.h265>
//!
//! Output: per-frame line + aggregate at the end.
//!   `pb_intra` is the heuristic the current pipeline uses
//!   (`intra_pixels / total_pixels` derived from PbInfo.ref_poc0 == -1).
//!   `cb_intra` reads PredMode directly. `cb_skip` is new (MODE_SKIP
//!   share — cannot be derived from PB info).

use std::env;
use std::fs::File;
use std::io::Read;
use std::path::Path;

use fovea_mv_stream::hevc::{HevcDecoder, HevcResult};

fn main() -> HevcResult<()> {
    let args: Vec<String> = env::args().collect();
    let path = args.get(1).cloned().unwrap_or_else(|| {
        // Default to the committed HEVC sample if none given.
        let manifest = env::var("CARGO_MANIFEST_DIR").unwrap();
        Path::new(&manifest)
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("benchmarks/datasets/cctv-sample-hevc/sample.h265")
            .display()
            .to_string()
    });

    eprintln!("clip: {path}");

    let mut buf = Vec::new();
    File::open(&path)
        .expect("open .h265 file")
        .read_to_end(&mut buf)
        .expect("read .h265 file");
    eprintln!("bytes: {}", buf.len());

    let mut dec = HevcDecoder::new()?;
    dec.push(&buf)?;
    dec.flush()?;

    println!(
        "{:>5} {:>10} {:>4} {:>10} {:>10} {:>10} {:>10} {:>10}",
        "frame", "slice_t", "pbW", "pb_intra", "cb_intra", "cb_skip", "cb_inter", "diff"
    );

    let mut frame_idx = 0u32;
    let mut sum_pb = 0.0f64;
    let mut sum_cb = 0.0f64;
    let mut sum_diff_abs = 0.0f64;
    let mut max_diff_abs = 0.0f32;
    let mut samples = 0u64;

    loop {
        let (more, frame_opt) = dec.decode_step()?;
        let has_frame = frame_opt.is_some();

        if let Some(frame) = frame_opt {
            let (pb_w, pb_h, log2u) = frame.pb_layout();
            let pb_count = (pb_w as u64) * (pb_h as u64);
            let pixels_per_unit_sq = 1u64 << (2 * log2u);
            let total_pixels = pb_count * pixels_per_unit_sq;

            // Heuristic: count PBs with no L0/L1 reference as "intra-like",
            // weight by PB cell area, divide by total. Matches what
            // `lib.rs::handle_hevc_frame` does today.
            let pb_infos = frame.pb_info();
            let mut pb_intra_pixels: u64 = 0;
            for pb in &pb_infos {
                if pb.ref_poc0 == -1 && pb.ref_poc1 == -1 {
                    pb_intra_pixels += pixels_per_unit_sq;
                }
            }
            let pb_intra = if total_pixels == 0 {
                0.0
            } else {
                pb_intra_pixels as f32 / total_pixels as f32
            };

            // Direct: PredMode from cb_info via the new accessor.
            let cb = frame.cb_stats();
            let cb_intra = cb.intra_ratio();
            let cb_skip = cb.skip_ratio();
            let cb_inter = if cb.total_pixels == 0 {
                0.0
            } else {
                cb.inter_pixels as f32 / cb.total_pixels as f32
            };

            let diff = cb_intra - pb_intra;
            let diff_abs = diff.abs();
            sum_pb += pb_intra as f64;
            sum_cb += cb_intra as f64;
            sum_diff_abs += diff_abs as f64;
            if diff_abs > max_diff_abs {
                max_diff_abs = diff_abs;
            }
            samples += 1;

            println!(
                "{:>5} {:>10} {:>4} {:>10.4} {:>10.4} {:>10.4} {:>10.4} {:>+10.4}",
                frame_idx,
                cb.slice_type_first,
                pb_w,
                pb_intra,
                cb_intra,
                cb_skip,
                cb_inter,
                diff
            );
            frame_idx += 1;
        }

        if !more && !has_frame {
            // Some streams need an extra empty step after flush before
            // the loop drains. Try one more then bail.
            let (more2, frame2) = dec.decode_step()?;
            if frame2.is_none() && !more2 {
                break;
            }
        }
    }

    eprintln!();
    eprintln!("--- summary over {samples} frames ---");
    if samples > 0 {
        let mean_pb = sum_pb / samples as f64;
        let mean_cb = sum_cb / samples as f64;
        let mean_diff_abs = sum_diff_abs / samples as f64;
        eprintln!("mean pb_intra (heuristic)   = {:.6}", mean_pb);
        eprintln!("mean cb_intra (direct)      = {:.6}", mean_cb);
        eprintln!("mean |cb_intra - pb_intra|  = {:.6}", mean_diff_abs);
        eprintln!("max  |cb_intra - pb_intra|  = {:.6}", max_diff_abs);
    }

    Ok(())
}

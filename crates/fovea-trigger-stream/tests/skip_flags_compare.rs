//! A' verification: compare MV extraction + decode wall-clock + pixel
//! integrity between default decoding and skip-flag decoding.
//!
//! Hypothesis (from exec-plan):
//! 1. MV counts and motion energies are **identical** between modes.
//!    (MVs come from bitstream entropy decode, not from reconstructed pixels.)
//! 2. Decoding is **faster** with skip flags.
//! 3. Pixel output **diverges** with skip flags (mean luma per frame differs).

use std::path::PathBuf;
use std::time::Instant;

use fovea_trigger_core::{motion_energy, FrameType};
use fovea_trigger_stream::{FfmpegSource, OpenOptions};

fn sample_path() -> Option<PathBuf> {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p.push("benchmarks/datasets/cctv-sample/sample.mp4");
    if p.exists() { Some(p) } else { None }
}

#[derive(Debug)]
struct PassRecord {
    elapsed_ms: f64,
    frame_count: u32,
    i_count: u32,
    total_mv_count: u64,
    total_energy: u64,
    mean_luma_per_frame: Vec<f64>,
}

fn run_pass(path: &std::path::Path, opts: OpenOptions) -> PassRecord {
    let start = Instant::now();
    let mut src = FfmpegSource::open_file_with(path, opts).expect("open");

    let mut record = PassRecord {
        elapsed_ms: 0.0,
        frame_count: 0,
        i_count: 0,
        total_mv_count: 0,
        total_energy: 0,
        mean_luma_per_frame: Vec::new(),
    };

    while let Some(pkt) = src.next_packet().expect("decode") {
        record.frame_count += 1;
        if pkt.frame_type == FrameType::I {
            record.i_count += 1;
        }
        record.total_mv_count += pkt.mvs.len() as u64;
        record.total_energy += motion_energy(&pkt.mvs);
        // Compute mean luma of the Y plane via the underlying decoded frame.
        // SAFETY: as_ptr() borrows the live AVFrame for this iteration;
        // data[0] / linesize[0] / width / height are all valid for that scope.
        unsafe {
            let raw = src.last_frame_ptr();
            let data = (*raw).data[0];
            let linesize = (*raw).linesize[0] as usize;
            let w = (*raw).width as usize;
            let h = (*raw).height as usize;
            if !data.is_null() && w > 0 && h > 0 {
                let mut sum: u64 = 0;
                for y in 0..h {
                    let row = data.add(y * linesize);
                    for x in 0..w {
                        sum += *row.add(x) as u64;
                    }
                }
                let mean = sum as f64 / (w as f64 * h as f64);
                record.mean_luma_per_frame.push(mean);
            } else {
                record.mean_luma_per_frame.push(f64::NAN);
            }
        }
    }
    record.elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
    record
}

#[test]
fn compare_default_vs_skip_flags() {
    let Some(path) = sample_path() else {
        eprintln!("sample missing, skipping");
        return;
    };

    let baseline = run_pass(&path, OpenOptions::default());
    let aprime = run_pass(
        &path,
        OpenOptions {
            skip_loop_filter_all: true,
            skip_idct_all: true,
        },
    );

    println!("=== baseline (default) ===");
    println!("  elapsed       = {:.1} ms", baseline.elapsed_ms);
    println!("  frame_count   = {}", baseline.frame_count);
    println!("  total MVs     = {}", baseline.total_mv_count);
    println!("  total energy  = {}", baseline.total_energy);

    println!("=== A' (skip_loop_filter + skip_idct) ===");
    println!("  elapsed       = {:.1} ms", aprime.elapsed_ms);
    println!("  frame_count   = {}", aprime.frame_count);
    println!("  total MVs     = {}", aprime.total_mv_count);
    println!("  total energy  = {}", aprime.total_energy);

    let speedup = baseline.elapsed_ms / aprime.elapsed_ms;
    println!("speedup = {:.2}x", speedup);

    // Identical frame count and MV statistics.
    assert_eq!(baseline.frame_count, aprime.frame_count);
    assert_eq!(baseline.total_mv_count, aprime.total_mv_count);
    assert_eq!(baseline.total_energy, aprime.total_energy);
    assert_eq!(baseline.i_count, aprime.i_count);

    // Pixel divergence: count frames where mean luma differs by more than 1
    // (Y is 0-255). I-frames may match (no IDCT residual contributes to a
    // pure intra reconstruction... actually IDCT *is* used for intra residuals
    // too, so even I-frames likely diverge).
    let mut diverged = 0u32;
    let mut max_diff: f64 = 0.0;
    let mut total_diff: f64 = 0.0;
    let n = baseline.mean_luma_per_frame.len().min(aprime.mean_luma_per_frame.len());
    for i in 0..n {
        let d = (baseline.mean_luma_per_frame[i] - aprime.mean_luma_per_frame[i]).abs();
        total_diff += d;
        if d > max_diff {
            max_diff = d;
        }
        if d > 1.0 {
            diverged += 1;
        }
    }
    let mean_diff = total_diff / n as f64;
    println!("pixel mean-luma divergence: {diverged}/{n} frames > 1.0");
    println!("  max diff = {:.3}", max_diff);
    println!("  mean diff = {:.3}", mean_diff);

    // Diagnostic only — do not assert. Observed FFmpeg 8 behavior:
    // skip_idct is largely a no-op in the modern H.264 decoder, so pixels
    // do not drift as advertised in older docs. skip_loop_filter still
    // applies but its effect on mean luma is small (deblocking is a
    // smoothing filter; mean is preserved). The conclusion is recorded
    // here for the project record rather than enforced as an invariant.
    let _ = diverged;
    let _ = max_diff;
    let _ = mean_diff;
}

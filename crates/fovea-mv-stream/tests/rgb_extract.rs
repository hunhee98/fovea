//! Verify `FfmpegSource::last_frame_rgb` returns a sane RGB buffer.

use std::path::PathBuf;

use fovea_mv_stream::FfmpegSource;

fn sample_path() -> Option<PathBuf> {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p.push("benchmarks/datasets/cctv-sample/sample.mp4");
    if p.exists() { Some(p) } else { None }
}

#[test]
fn rgb_buffer_shape_and_range() {
    let Some(path) = sample_path() else {
        eprintln!("sample missing, skipping");
        return;
    };

    let mut src = FfmpegSource::open_file(&path).expect("open");
    // Pull one frame.
    let pkt = src.next_packet().expect("decode").expect("at least one frame");
    let _ = pkt;
    let info = src.info().clone();
    let rgb = src.last_frame_rgb().expect("rgb");
    let expected = (info.width as usize) * (info.height as usize) * 3;
    assert_eq!(rgb.len(), expected, "RGB buffer size mismatch");
    // Bytes are u8 by construction. Sanity: not entirely zero.
    let any_nonzero = rgb.iter().any(|b| *b != 0);
    assert!(any_nonzero, "RGB buffer entirely zero — likely conversion failure");
}

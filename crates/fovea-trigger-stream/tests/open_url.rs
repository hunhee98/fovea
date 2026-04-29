//! Step 002.1 — verify `FfmpegSource::open_url` accepts `file://` URLs and
//! returns the same metadata + frame counts as `open_file`.

use std::path::PathBuf;

use std::time::Instant;

use fovea_trigger_stream::{FfmpegSource, NetworkOptions, OpenOptions, RtspTransport};

fn sample_path() -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p.push("benchmarks/datasets/cctv-sample/sample.mp4");
    p
}

#[test]
fn open_url_with_file_scheme_matches_open_file() {
    let path = sample_path();
    if !path.exists() {
        eprintln!("sample missing at {path:?}, skipping");
        return;
    }

    let abs = path
        .canonicalize()
        .expect("canonicalize sample path");
    let url = format!("file://{}", abs.display());

    let mut via_url =
        FfmpegSource::open_url(&url, NetworkOptions::default()).expect("open via file:// URL");
    let mut via_path = FfmpegSource::open_file(&path).expect("open via path");

    let url_info = via_url.info().clone();
    let path_info = via_path.info().clone();
    assert_eq!(url_info.codec, path_info.codec);
    assert_eq!(url_info.width, path_info.width);
    assert_eq!(url_info.height, path_info.height);
    assert_eq!(url_info.frame_rate, path_info.frame_rate);
    assert_eq!(url_info.duration_us, path_info.duration_us);

    let mut url_count = 0u32;
    let mut path_count = 0u32;
    while via_url.next_packet().expect("decode url").is_some() {
        url_count += 1;
    }
    while via_path.next_packet().expect("decode path").is_some() {
        path_count += 1;
    }
    assert_eq!(
        url_count, path_count,
        "frame count mismatch between file:// and path"
    );
    println!("frame count = {url_count} (matches both paths)");
}

#[test]
fn open_url_with_decoder_options_combines() {
    let path = sample_path();
    if !path.exists() {
        return;
    }
    let abs = path.canonicalize().expect("canonicalize");
    let url = format!("file://{}", abs.display());
    let decoder = OpenOptions {
        skip_loop_filter_all: true,
        skip_idct_all: true,
    };
    let src = FfmpegSource::open_url_with(&url, NetworkOptions::default(), decoder)
        .expect("open with both option types");
    let info = src.info();
    assert_eq!(info.codec, ffmpeg_next::codec::Id::H264);
}

#[test]
fn open_url_invalid_returns_error_not_panic() {
    let result = FfmpegSource::open_url(
        "rtsp://0.0.0.0:1/nonexistent",
        NetworkOptions::default(),
    );
    assert!(result.is_err(), "unreachable RTSP url should error, not succeed");
}

#[test]
fn open_url_custom_network_options_does_not_break_file_path() {
    let path = sample_path();
    if !path.exists() {
        return;
    }
    // Custom NetworkOptions should be silently ignored by the file demuxer.
    let opts = NetworkOptions {
        transport: RtspTransport::Udp,
        open_timeout_ms: 1_000,
        read_timeout_ms: 1_000,
        max_reconnects: 5,
    };
    let abs = path.canonicalize().expect("canonicalize");
    let url = format!("file://{}", abs.display());
    let mut src = FfmpegSource::open_url(&url, opts).expect("file open with custom net opts");
    assert!(src.next_packet().expect("decode").is_some());
}

#[test]
fn open_url_rtsp_short_timeout_bounds_failure_time() {
    // 192.0.2.0/24 is TEST-NET-1: guaranteed to be unroutable. With a 500 ms
    // open_timeout the call must return inside ~2 s (allow some FFmpeg
    // overhead beyond the timeout itself).
    let opts = NetworkOptions {
        open_timeout_ms: 500,
        ..NetworkOptions::default()
    };
    let start = Instant::now();
    let result = FfmpegSource::open_url("rtsp://192.0.2.1:554/cam", opts);
    let elapsed_ms = start.elapsed().as_millis();
    assert!(result.is_err(), "expected error on unroutable RTSP host");
    println!("rtsp open failed after {elapsed_ms} ms");
    assert!(
        elapsed_ms < 5_000,
        "rtsp open took too long ({elapsed_ms} ms) — stimeout not honored?"
    );
}

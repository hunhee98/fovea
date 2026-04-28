//! Step 2.8 property test.
//!
//! Hands `FfmpegSource::open_file` random bytes via a temp file and asserts
//! we get a `Result::Err` (not a panic) for malformed input. FFmpeg's
//! demuxers are battle-tested, but this guards the **boundary** code in our
//! crate against panics on unexpected input.
//!
//! Coverage: short random buffers (typical fuzz seed sizes). We do not try
//! to feed valid-looking H.264 streams; that's covered by golden + sample.

use std::io::Write;

use proptest::prelude::*;
use proptest::test_runner::Config;

use fovea_mv_stream::FfmpegSource;

fn write_temp(bytes: &[u8]) -> std::path::PathBuf {
    use std::time::SystemTime;
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let path = std::env::temp_dir().join(format!("fovea-fuzz-{}-{}.bin", std::process::id(), nanos));
    let mut f = std::fs::File::create(&path).expect("create temp");
    f.write_all(bytes).expect("write temp");
    path
}

proptest! {
    #![proptest_config(Config::with_cases(64))]

    #[test]
    fn open_random_bytes_does_not_panic(bytes in prop::collection::vec(any::<u8>(), 0..=2048)) {
        let path = write_temp(&bytes);
        // We expect an error (or, very rarely, a successful open if the bytes
        // happened to look like a valid container — but then the iterator
        // must also not panic). In all cases, no panic.
        let result = std::panic::catch_unwind(|| FfmpegSource::open_file(&path));
        let _ = std::fs::remove_file(&path);
        prop_assert!(result.is_ok(), "open panicked on random bytes");
        if let Ok(Ok(mut src)) = result {
            // If somehow it opened, draining packets must not panic either.
            let drain = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                for _ in 0..32 {
                    if src.next_packet().ok().flatten().is_none() {
                        break;
                    }
                }
            }));
            prop_assert!(drain.is_ok(), "next_packet panicked on random-bytes source");
        }
    }
}

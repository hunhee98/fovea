//! Benchmark per-packet processing through `FfmpegSource`.
//!
//! Reports wall time per `next_packet` averaged over a full pass of the
//! committed CCTV sample. Skips silently if the sample is missing.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use fovea_mv_stream::FfmpegSource;

fn sample_path() -> Option<PathBuf> {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop();
    p.pop();
    p.push("benchmarks/datasets/cctv-sample/sample.mp4");
    if p.exists() { Some(p) } else { None }
}

fn count_packets(path: &std::path::Path) -> u64 {
    let mut src = FfmpegSource::open_file(path).expect("open");
    let mut n = 0u64;
    while src.next_packet().expect("decode").is_some() {
        n += 1;
    }
    n
}

fn bench_full_pass(c: &mut Criterion) {
    let Some(path) = sample_path() else {
        eprintln!("sample missing, skipping ffmpeg_source bench");
        return;
    };

    let total_packets = count_packets(&path);
    eprintln!("sample packet count = {total_packets}");

    let mut group = c.benchmark_group("ffmpeg_source");
    group.throughput(Throughput::Elements(total_packets));
    // Each iteration opens, decodes the full clip, drops. Reports per-iteration
    // total time; divide by total_packets for per-packet wall time.
    group.sample_size(10);
    group.measurement_time(Duration::from_secs(15));
    group.bench_function("full_pass_decode_with_mvs", |b| {
        b.iter_custom(|iters| {
            let mut total = Duration::ZERO;
            for _ in 0..iters {
                let start = Instant::now();
                let mut src = FfmpegSource::open_file(&path).expect("open");
                let mut sum: u64 = 0;
                while let Some(pkt) = src.next_packet().expect("decode") {
                    sum = sum.wrapping_add(pkt.mvs.len() as u64);
                }
                black_box(sum);
                total += start.elapsed();
            }
            total
        });
    });
    group.finish();
}

criterion_group!(benches, bench_full_pass);
criterion_main!(benches);

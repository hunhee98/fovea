//! Benchmark for `motion_energy` aggregation.
//!
//! Sizes mirror what we expect per packet on real content:
//! - 1024  ≈ small clip / low-motion 720p P-slice
//! - 8192  ≈ 1080p P-slice baseline (typical inter MB count w/o sub-MB blowup)
//! - 20000 ≈ 1080p B-slice with sub-MB partitions and bipred (observed in
//!   the committed sample, max 19,508 MVs/packet)

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use fovea_mv_core::{motion_energy, MotionVector};

fn make_mvs(n: usize) -> Vec<MotionVector> {
    (0..n)
        .map(|i| MotionVector {
            w: 16,
            h: 16,
            src_x: 0,
            src_y: 0,
            dst_x: 0,
            dst_y: 0,
            // Vary so the optimizer can't constant-fold.
            motion_x: ((i as i32) % 31) - 15,
            motion_y: ((i as i32) % 17) - 8,
            motion_scale: 4,
            source: -1,
        })
        .collect()
}

fn bench_motion_energy(c: &mut Criterion) {
    let mut group = c.benchmark_group("motion_energy");
    for n in [1024usize, 8192, 20000] {
        let mvs = make_mvs(n);
        group.throughput(Throughput::Elements(n as u64));
        group.bench_with_input(BenchmarkId::from_parameter(n), &mvs, |b, mvs| {
            b.iter(|| black_box(motion_energy(black_box(mvs))));
        });
    }
    group.finish();
}

criterion_group!(benches, bench_motion_energy);
criterion_main!(benches);

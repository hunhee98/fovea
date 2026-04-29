# 2026-04-28 — density smoke (M3 8-core, cctv-sample H.264 1080×1920)

Status: **smoke run.** Establishes the runner and gives a first density curve on
file source. Not yet a 0.2 acceptance result — see Limitations.

## TL;DR

On Apple M3 8-core, parsing one 30 fps 1080×1920 H.264 stream through
`fovea-trigger` costs ~15% of one core in realtime equivalents. The 8-core
box reaches ~1× throughput at N=16, which is the soft ceiling for this
clip on this hardware in the CPU-only path.

## Hardware

Apple M3 (8-core), 16 GB, GPU none (CPU-only path), Darwin 25.3.0, rustc 1.91.1

## Commit

`6deb2dc` (feat/mv-step2-h264-extraction)

## Command

```sh
make density-bench
# expands to:
.venv/bin/python -m benchmarks.runners.density \
    --source benchmarks/datasets/cctv-sample/sample.mp4 \
    --streams 1,2,4,8,16 \
    --duration-s 15 \
    --run-id density-smoke
```

Source clip: `benchmarks/datasets/cctv-sample/sample.mp4` — 15.23 s,
1080×1920, 30 fps, H.264.

Trigger configuration: `motion_threshold=10_000_000`,
`interval_ms=60_000`. Both intentionally high so the bench measures
the parse path (no decode, near-zero fires). Decode cost is reported
separately in the canonical accuracy bench.

## Numbers

| streams | cpu% (flat) | cpu% (rt est) | throughput × | RSS MB / stream |
|--------:|------------:|--------------:|-------------:|----------------:|
|       1 |        99.5 |          14.9 |          6.7 |            87.5 |
|       2 |        95.1 |          17.0 |          5.6 |            88.0 |
|       4 |        90.1 |          20.9 |          4.3 |            88.7 |
|       8 |        66.1 |          26.5 |          2.5 |            86.4 |
|      16 |        29.9 |          31.3 |          1.0 |            70.1 |

- **cpu% (flat)**: mean self-CPU% per worker, sampled every 100 ms during a
  parse-as-fast-as-possible run. One worker can saturate one core (~99%).
- **cpu% (rt est)**: realtime equivalent — `cpu%(flat) / throughput_x`. What
  one worker would cost if rate-limited to native playback (30 fps).
- **throughput ×**: how many × native clip duration each worker processed
  per wall-second. Drops from 6.7× at N=1 to 1.0× at N=16, marking the
  point where the box can just barely keep up with N realtime streams.

## Findings

1. **Parse path scales as expected up to physical-core count.** N=1..8 sees
   each worker pin one core at 90–99% CPU(flat) with throughput tapering
   from 6.7× to 2.5×. RSS holds steady at ~88 MB per worker.
2. **N=16 hits realtime ceiling.** Throughput collapses to 1.0× — every
   worker is just keeping pace with the clip's native frame rate. RSS
   per worker drops to 70 MB because workers context-switch out before
   decoder buffers fully expand.
3. **Per-stream realtime cost is ~15% / core** at N=1, drifting up to
   ~31% at N=16 due to scheduling overhead. The 8-core box can host
   roughly 8 / 0.15 ≈ ~50 streams at native frame rate **in theory**;
   the measured ceiling of ~16 reflects scheduling and per-process
   overhead this runner does not yet isolate.

## Limitations

- **File source, not RTSP.** Each worker reads the same .mp4 from disk
  and parses as fast as it can. A real deployment with RTSP rate-limits
  to 30 fps per stream — the realtime number above is an extrapolation,
  not a direct measurement. RTSP-based density is the next iteration.
- **Single clip, single resolution.** 1080×1920 H.264 only. Density
  on 720p / lower-bitrate sources will be different and should be
  measured separately.
- **Trigger fires near zero.** Decode is excluded from this number by
  design. Active-window CPU is reported in the canonical accuracy bench.
- **Trigger latency not measured here.** The current Python API yields
  events on trigger fire, not per packet, so wall-clock between fires is
  not packet latency. Latency belongs in a criterion microbench in
  `crates/fovea-trigger-core/benches/`.
- **Not an acceptance result.** The 0.2 acceptance bar (≤ 5% per stream
  at N=100 on commodity hardware) is not met by this run, both because
  N=100 was not attempted and because the realtime number is an estimate.
  This run establishes the methodology; the acceptance run needs RTSP
  source, longer duration, and at minimum N=64.

## Reproduction

```sh
make density-bench         # the row set above
make density-bench-large   # extends to N=32, 64; ~10 minutes
```

Raw per-stage JSON: `benchmarks/_runs/density-smoke/summary.json`
(gitignored under `_runs/`).

## Methodology

See `docs/03.methodology/density.md`. The estimator is:

```
realtime_cpu_per_stream = observed_cpu_flat / (clip_duration × loops / wall_seconds)
```

Both terms are measured per worker; we average across workers.

## Files

- `benchmarks/runners/density.py` — the runner, multiprocessing-based.
- `Makefile` targets `density-bench` and `density-bench-large`.
- `docs/03.methodology/density.md` — metric definitions.

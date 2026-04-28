# 2026-04-29 — pixel-diff baseline vs fovea-mv density (M3 8-core, cctv-sample)

Status: smoke comparison. First side-by-side density curve against the
classical OSS motion detector. Same hardware, same clip, same stream
counts, same instrumentation as `2026-04-28-density-smoke.md`.

## TL;DR

On Apple M3 (8-core) parsing the same 30 fps 1080p H.264 clip, fovea-mv
costs **~14.9% of one core per realtime stream**; the textbook
pixel-diff baseline (decode + grayscale + frame diff) costs
**~30.1% of one core per realtime stream** — about **2× more CPU**
with about **2× more RSS** (172 MB vs 88 MB per worker). Both saturate
the box at N=16 realtime streams on this clip; at that ceiling
fovea-mv has ~30% CPU headroom while pixel-diff is at 45%.

## Hardware

Apple M3 (8-core), 16 GB, GPU none (CPU-only path), Darwin 25.3.0,
OpenCV 4.13.0, rustc 1.91.1.

## Commit

`fc46368` (feat/mv-step2-h264-extraction).

## Source

`benchmarks/datasets/cctv-sample/sample.mp4` — the same 15.23 s,
1080×1920, H.264 30 fps clip used in the fovea-mv density smoke run.

## Command

```sh
# pixel-diff baseline
.venv/bin/python -m benchmarks.baselines.pixel_diff.density \
    --source benchmarks/datasets/cctv-sample/sample.mp4 \
    --streams 1,2,4,8,16 \
    --duration-s 15 \
    --threshold 1e15 \
    --run-id pixeldiff-smoke
```

`--threshold 1e15` is chosen so the trigger never fires; the cost we
measure is the decode + diff loop, equivalent to fovea-mv's "high
motion-threshold + long interval" parse-path baseline.

The fovea-mv numbers are reproduced verbatim from
`benchmarks/results/2026-04-28-density-smoke.md` for a like-for-like
table.

## Numbers

| streams | impl | cpu%(flat) | cpu%(rt est) | throughput × | RSS MB |
|--------:|------|-----------:|-------------:|-------------:|-------:|
|    1 | pixel-diff | 458.8 | **30.1** | 15.23 | 172.4 |
|    1 | fovea-mv   |  99.5 | **14.9** |  6.71 |  87.5 |
|    2 | pixel-diff | 329.0 | **36.0** |  9.13 | 198.9 |
|    2 | fovea-mv   |  95.1 | **17.0** |  5.60 |  88.0 |
|    4 | pixel-diff | 153.8 | **37.9** |  4.06 | 177.3 |
|    4 | fovea-mv   |  90.1 | **20.9** |  4.30 |  88.7 |
|    8 | pixel-diff |  87.3 | **43.0** |  2.03 | 178.8 |
|    8 | fovea-mv   |  66.1 | **26.5** |  2.50 |  86.4 |
|   16 | pixel-diff |  45.4 | **44.9** |  1.01 | 140.8 |
|   16 | fovea-mv   |  29.9 | **31.3** |  1.00 |  70.1 |

`cpu%(rt est) = cpu%(flat) / throughput_x` — what one worker would
cost if rate-limited to the clip's native frame rate (30 fps). It is
the comparable "per realtime stream" cost; the flat-CPU column is
"how many cores does each worker pin while running flat-out", which
differs because OpenCV's H.264 decode is multi-threaded (one
pixel-diff worker uses ~4–5 cores at N=1) while fovea-mv's parse path
is single-threaded.

## Findings

1. **Per-realtime-stream CPU cost: fovea-mv ~2× cheaper across the
   board.** At N=1 it is 14.9% vs 30.1%; the gap stays around 1.4–2×
   through N=16.
2. **RSS: fovea-mv ~2× smaller per worker.** 70–88 MB vs 140–200 MB.
   Mostly OpenCV's decode buffers + intermediate gray frames.
3. **Realtime ceiling on this box and clip: ~16 for both.** Throughput
   collapses to 1.0× at N=16 in both implementations — the ceiling is
   set by physical cores plus scheduling, not by the per-stream cost
   gap.
4. **Headroom at the ceiling.** At N=16 each fovea-mv worker uses
   31.3% of a realtime stream's CPU budget, vs 44.9% for pixel-diff.
   On a wider box or a longer clip, fovea-mv's lower per-stream cost
   should let it host meaningfully more concurrent streams before
   saturating.

## Limitations

- **Single clip, single resolution.** 1080×1920 H.264 only, 15 s.
  Cheaper / 720p / lower-bitrate sources change both numbers; bench
  separately if making per-resolution claims.
- **Single hardware.** M3 only. NVDEC / VideoToolbox decode acceleration
  is not exercised; the pixel-diff decode path is CPU only.
- **`--threshold 1e15` means zero fires for pixel-diff.** Latency and
  recall numbers are not produced here; this is a cost-only comparison
  matching the fovea-mv smoke setup. Trigger-quality numbers (FP rate,
  recall@event) belong in the dedicated PTZ / dynamicBackground /
  UCF-Crime benches (P0.4, P0.5, P0.7 in
  `docs/05.exec-plans/004-positioning.md`).
- **OpenCV multi-threaded decode skews flat CPU%.** The "cpu%(flat)"
  column for pixel-diff exceeds 100% per worker because libavcodec
  runs decode threads inside the worker process. The realtime-equivalent
  column (cpu%(rt est)) is the apples-to-apples number; flat CPU% is
  there for context only.

## Reproduction

```sh
make verify-py                              # ensure deps installed
.venv/bin/python -m benchmarks.baselines.pixel_diff.density \
    --source benchmarks/datasets/cctv-sample/sample.mp4 \
    --streams 1,2,4,8,16 \
    --duration-s 15 \
    --threshold 1e15 \
    --run-id pixeldiff-smoke

# Compare against:
make density-bench   # fovea-mv equivalent, see 2026-04-28-density-smoke.md
```

Raw per-stage JSON:
- `benchmarks/_runs/pixeldiff-smoke/summary.json` (gitignored under `_runs/`)
- `benchmarks/_runs/density-smoke/summary.json` (same, fovea-mv side)

## Files

- `benchmarks/baselines/pixel_diff/density.py` — the runner
- `benchmarks/baselines/pixel_diff/__init__.py`
- `benchmarks/baselines/__init__.py`

## What's next

- Sweep `--threshold` against the same clip with normal traffic motion
  to characterize the pixel-diff false-positive curve, and run the
  fovea-mv MotionTrigger over the same source for an apples-to-apples
  trigger-quality comparison.
- Run on the Seattle DOT live HLS clip (`benchmarks/datasets/seattle-dot`)
  to confirm the numbers carry over from the academic test clip to
  real production CCTV bitstream.
- N=32, N=64 sweeps to find the ceiling difference on a wider box.

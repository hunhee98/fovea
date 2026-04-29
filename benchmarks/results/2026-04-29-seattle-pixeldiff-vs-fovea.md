# 2026-04-29 — Seattle DOT (real CCTV): pixel-diff vs fovea-trigger density

Status: smoke comparison on a real production CCTV bitstream, paired
with `2026-04-29-pixeldiff-vs-fovea-density.md` (academic clip).
Confirms the fovea-trigger vs pixel-diff cost ratio holds — and grows — on
a real municipal-CCTV encoder.

## TL;DR

On the same M3 8-core box, parsing a 60 s 1080p H.264 clip captured
from a Seattle DOT public traffic camera (Wowza-fronted municipal
encoder, GOP ≈ 15, no B-frames, ~3 Mbps): fovea-trigger costs **5.3% of
one core per realtime stream** vs **15.4% for the pixel-diff
baseline** — a **2.9× gap**. On the academic `cctv-sample` clip the
gap was 2.0×; the real-CCTV bitstream is more favorable to the
parse path.

## Hardware

Apple M3 (8-core), 16 GB, GPU none (CPU-only path), Darwin 25.3.0,
OpenCV 4.13.0, rustc 1.91.1.

## Commit

`6527f67` (feat/mv-step2-h264-extraction).

## Source

`benchmarks/datasets/seattle-dot/data/24_NW_Market_EW_60s.mp4` —
60 s capture from
`https://61e0c5d388c2e.streamlock.net/live/24_NW_Market_EW.stream/playlist.m3u8`,
fetched via `benchmarks/datasets/seattle-dot/download.sh` with
`-c copy` so the original bitstream is preserved (no local re-encode).

Verified by ffprobe at capture time:

```
codec    : H.264 High @ Level 4.0
size     : 1920 × 1080
fps      : 30 (declared) / ~22 (effective after HLS chunking)
bitrate  : ~3.0 Mbps
GOP      : ~15 frames (88 I + 1233 P over 60 s)
B-frames : 0
```

This matches typical municipal-ITS encoder profiles (Axis / Bosch class
behind Wowza). Not a phone camera, not a re-encoded academic clip.

## Command

```sh
# Capture (one-time, 22 MB)
DURATION_S=60 benchmarks/datasets/seattle-dot/download.sh

# fovea-trigger side, full sweep
.venv/bin/python -m benchmarks.runners.density \
    --source benchmarks/datasets/seattle-dot/data/24_NW_Market_EW_60s.mp4 \
    --streams 1,2,4,8,16 \
    --duration-s 30 \
    --run-id density-seattle

# pixel-diff side, light sweep (MBA thermals — see Limitations)
.venv/bin/python -m benchmarks.baselines.pixel_diff.density \
    --source benchmarks/datasets/seattle-dot/data/24_NW_Market_EW_60s.mp4 \
    --streams 1,2,4 \
    --duration-s 20 \
    --threshold 1e15 \
    --run-id pixeldiff-seattle
```

## Numbers

### fovea-trigger (N=1,2,4,8,16, duration 30 s)

| streams | cpu%(flat) | cpu%(rt est) | throughput × | RSS MB |
|--------:|-----------:|-------------:|-------------:|-------:|
|       1 |       99.1 |          5.3 |         18.8 |   69.1 |
|       2 |       97.1 |          5.7 |         17.1 |   68.0 |
|       4 |       95.7 |          7.5 |         12.7 |   66.9 |
|       8 |       75.3 |          9.5 |          7.9 |   64.7 |
|      16 |       39.3 |         10.6 |          3.7 |   57.4 |

### pixel-diff (N=1,2,4, duration 20 s, light sweep)

| streams | cpu%(flat) | cpu%(rt est) | throughput × | RSS MB |
|--------:|-----------:|-------------:|-------------:|-------:|
|       1 |      415.5 |         15.4 |         27.0 |  158.3 |
|       2 |      246.2 |         16.4 |         15.0 |  174.8 |
|       4 |      136.1 |         15.1 |          9.0 |  155.0 |

### Per-realtime-stream cost ratio (Seattle vs cctv-sample)

| clip | N=1 pd / fovea | N=2 pd / fovea | N=4 pd / fovea |
|---|---:|---:|---:|
| cctv-sample (academic) | 2.02× | 2.12× | 1.81× |
| **Seattle DOT (real CCTV)** | **2.91×** | **2.88×** | **2.01×** |

## Findings

1. **fovea-trigger ~3× cheaper than pixel-diff on a real CCTV bitstream**
   at N=1, vs ~2× on the academic clip. The wedge gets larger on real
   CCTV.
2. **Likely reason**: Seattle's encoder uses a long GOP and zero
   B-frames, which trims fovea-trigger's parse-path work; pixel-diff's
   decode cost is ~constant in clip codec choice. The academic clip
   is encoded with default x264 settings (closer GOP, B-frames
   enabled), which costs fovea-trigger a bit more but doesn't help
   pixel-diff.
3. **RSS gap holds**: ~2.3× more memory per pixel-diff worker
   (155–175 MB vs 64–69 MB).
4. **Realtime throughput at N=1**: pixel-diff 27.0× vs fovea-trigger
   18.8× — pixel-diff is faster *flat-out* because OpenCV's H.264
   decode is multi-threaded inside a single process, while fovea-trigger
   parses single-threaded. The realtime-equivalent column normalizes
   that out.

## Limitations

- **Pixel-diff sweep stops at N=4** because the OpenCV decode threads
  per worker fan out across cores and N≥8 saturates the M3 + thermal
  throttles, producing meaningless absolute numbers (cf. the N=8/16
  rows of `2026-04-29-pixeldiff-vs-fovea-density.md`). Headline
  numbers at higher N belong on a wider, fanned cloud box.
- **Single Seattle camera, 60 s capture**. Different intersections may
  produce different bitstreams; longer capture for a soak claim is a
  separate run.
- **Trigger never fires** (`--threshold 1e15`). Trigger-quality
  numbers (FP, recall) are out of scope for this comparison; see
  P0.4 / P0.5 / P0.7 in `docs/05.exec-plans/004-positioning.md`.
- **HLS source** — capture latency (1–2 s segment buffer) does not
  affect the offline parse measurements but is relevant when
  comparing live-trigger latency, which is not measured here.

## Reproduction

```sh
DURATION_S=60 benchmarks/datasets/seattle-dot/download.sh
make verify-py

.venv/bin/python -m benchmarks.runners.density \
    --source benchmarks/datasets/seattle-dot/data/24_NW_Market_EW_60s.mp4 \
    --streams 1,2,4,8,16 --duration-s 30 --run-id density-seattle

.venv/bin/python -m benchmarks.baselines.pixel_diff.density \
    --source benchmarks/datasets/seattle-dot/data/24_NW_Market_EW_60s.mp4 \
    --streams 1,2,4 --duration-s 20 --threshold 1e15 \
    --run-id pixeldiff-seattle
```

Raw per-stage JSON:
- `benchmarks/_runs/density-seattle/summary.json`
- `benchmarks/_runs/pixeldiff-seattle/summary.json`

## What's next

- Repeat at N=8, N=16 on a wider cloud box to retire the MBA thermal
  caveat from the pixel-diff side.
- Sweep `--threshold` low enough to fire on traffic motion, then
  measure FP rate on idle hours (e.g. 4 am Seattle) for the same
  source.
- Add a second intersection from
  `wttdotm/traffic_cam_photobooth/seattleSources.json` to confirm the
  ratio isn't a single-camera artefact.

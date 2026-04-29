# Compressed-domain motion-vector libraries — capability matrix

A side-by-side of the OSS choices for "get motion vectors out of an
encoded video stream and use them to gate downstream work". Captured
here so the project's README and result-file comparisons can be
specific about what fovea-trigger adds, rather than making vague
"X× faster" claims.

Last updated: 2026-04-29.

## Scope

The four projects below are the ones a developer is most likely to
reach for if the task is "decide which video frames the expensive
analytics layer should look at". They span three architectural tiers:

- **Pixel-domain post-decode** — Frigate's motion module. Decodes
  every frame, sums frame-to-frame absdiff, fires on threshold.
  Lives one layer above the bitstream.
- **Compressed-domain raw extraction** — ffmpeg's `+export_mvs`
  flag and the `mv-extractor` Python wrapper around it. Reads MV
  arrays out of `libavcodec` without needing pixel data.
- **Compressed-domain trigger engine** — fovea-trigger. Adds HEVC, intra /
  skip ratios, global-motion correction, spatial-cluster reasoning,
  RTSP reconnect handling, and a trigger API on top of the same
  `libavcodec` substrate.

## Matrix

| Capability | ffmpeg `+export_mvs` (direct) | mv-extractor [github](https://github.com/LukasBommes/mv-extractor) | Frigate motion ([github](https://github.com/blakeblackshear/frigate)) | **fovea-trigger** |
|---|---|---|---|---|
| **Codecs** | | | | |
| H.264 motion vectors | ✅ via `+export_mvs` | ✅ | n/a (post-decode) | ✅ via `+export_mvs` |
| HEVC motion vectors | ⚠️ flag exists, partial in mainline | ❌ explicitly H.264 / MPEG-4 part 2 only | n/a (post-decode) | ✅ via vendored libde265 + `de265_internals` |
| AV1 / VP9 | ❌ | ❌ | n/a | ❌ explicit error (per [`CLAUDE.md`](../../CLAUDE.md)) |
| **Signals exposed** | | | | |
| Per-block motion vector | ✅ | ✅ | ❌ scalar pixel-diff | ✅ |
| Intra-coded block ratio | ⚠️ derivable from frame type only | ⚠️ derivable | ❌ | ✅ direct via `MvPacket::intra_count` |
| MODE_SKIP ratio (HEVC) | ❌ | ❌ | ❌ | ✅ via `Event::skip_ratio` (commit `5137935` and earlier) |
| Global-motion (camera-pan) correction | ❌ | ❌ | ❌ | ✅ `GlobalMotionEstimator` (commit `eebe916`) |
| Spatial cluster / concentration | ❌ | ❌ | ⚠️ via configurable mask | ✅ `SpatialClusterTrigger` (commit `6a26893`) |
| **Trigger primitives** | | | | |
| Threshold + debounce | ❌ caller writes | ❌ caller writes | ✅ frame-diff threshold | ✅ `MotionTrigger`, `IntervalTrigger`, `SceneChangeTrigger`, `SpatialClusterTrigger`, `FusionTrigger` |
| Per-MB resolution-independent threshold | ❌ | ❌ | ⚠️ | ✅ `MotionTrigger::with_per_mb_threshold` |
| ROI mask | ❌ caller writes | ❌ caller writes | ✅ | ✅ `RegionMask` |
| Multi-signal fusion | ❌ | ❌ | ❌ | ✅ `FusionTrigger` (motion ∧/∨ intra, skip-suppress gate) |
| **Sources** | | | | |
| File (`.mp4`, `.mov`, raw NAL) | ✅ | ✅ | ✅ | ✅ |
| RTSP live, with reconnect | ⚠️ caller writes loop | ⚠️ caller writes loop | ✅ | ✅ `Stream.from_url` w/ `max_reconnects`, `read_timeout_s` |
| HLS / `.m3u8` | ✅ | ✅ | ✅ | ✅ (`-c copy` capture flow in `benchmarks/datasets/seattle-dot/`) |
| **Concurrency** | | | | |
| Multi-stream in one process | ✅ caller writes | ❌ Python GIL — needs N processes | ✅ via internal worker pool | ⚠️ today: caller forks; ✅ planned: Tokio scheduler (P0.3 in [004](../05.exec-plans/004-positioning.md)) |
| **Language / packaging** | C / C++, no Python | Cython wrapper, x86-64 Linux wheel only | Python + Go, container | Rust core, PyO3 wheel (linux x86-64 + aarch64, macOS arm64) |
| **License** | LGPL/GPL (FFmpeg) | MIT | MIT | Apache-2.0 |

## What this means in practice

The honest framing — "fovea-trigger 빠르다" was the wrong claim:

- **Single-stream H.264 motion-vector extraction throughput** between
  ffmpeg-direct, mv-extractor, and fovea-trigger is approximately equal at
  the `libavcodec` API. None of them invent a faster way to read
  the same bytes; the time is dominated by `libavcodec`'s parser, not
  the binding language.
- **Multi-stream throughput** is where the picture changes. mv-extractor
  is GIL-bound, so hosting N streams costs N OS processes. fovea-trigger's
  Rust core can host them on a single Tokio runtime once P0.3 lands;
  density-bench numbers from that work are the right place to claim a
  ratio.
- **What fovea-trigger exposes that the others don't** — HEVC, MODE_SKIP
  ratio, intra ratio direct from `cb_info`, global-motion correction,
  spatial-cluster trigger, fusion trigger, RTSP reconnect — is the
  positioning. The matrix above is the one we ship in the README.

## How this matrix is verified

- "✅" entries point to commits or files in this repo. Any reader
  can clone HEAD, run `cargo test -p fovea-trigger-core`, and verify
  every fovea-trigger claim end-to-end.
- ffmpeg / mv-extractor / Frigate entries reference upstream
  documentation and source. Where a feature is partial or workaround-
  shaped, the cell uses ⚠️ rather than ✅.
- A side run of `mv-extractor` on identical input is folded into
  the next cloud-bench session (see 004 P0.2). The result file lives
  at `benchmarks/results/<date>-mv-extractor-N1-sanity.md` once that
  session runs; it is expected to confirm "approximately equal at
  N=1, fovea-trigger pulls ahead at N>1".

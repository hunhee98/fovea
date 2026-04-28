# 004 — fovea-mv positioning + P0 wedge proof

Status: in progress — supersedes 003 sequence
Owner: @hunhee98
Subproject: `fovea-mv`

## Why this plan exists

003 sequenced fovea-stream (Phase 3) as the next major piece on top of an
unproven Phase 1. That order is wrong. fovea-mv has a working prototype
(001) and a multi-stream density smoke run (`benchmarks/results/2026-04-28-density-smoke.md`)
but no head-to-head numbers against pixel-diff or mv-extractor, no PTZ
handling, no hero example. Without those, "MV is better than the
alternatives" is an assertion, not a claim.

004 says: prove the fovea-mv wedge first, then decide whether Phase 2/3
are the right next investment.

## Positioning statement

fovea-mv is **a Rust-based multi-channel motion-vector trigger engine for
the decoder front-end of NVR/VMS systems.** Not a single-camera motion
detector. Not a research MV extractor.

Target users:
- OSS NVRs (Frigate, Viseron, AgentDVR) that want to host more channels
  per box.
- Integrators building custom VMS for sites with PTZ cameras.
- Anyone running a VLM cascade where pre-decode gating saves real money.

The wedge against existing tools:

| Existing tool | Layer | Gap fovea-mv targets |
|---|---|---|
| ffmpeg `+export_mvs` | C API, raw MV access | no trigger primitives, no streaming integration, awkward to embed |
| mv-extractor (LukasBommes) | Python wrapper of ffmpeg | GIL-bound single-stream, file-oriented, no trigger logic |
| Frigate motion detector | pixel-diff in app | post-decode (decode cost paid up front), no global-motion handling, app-coupled |
| OpenCV background subtraction | pixel-domain | post-decode, no encoder-side info |

fovea-mv differentiates on three axes simultaneously:
1. **Pre-decode gating** (bitstream → decision, no full decode in idle path).
2. **Multi-channel concurrent** (Rust + no-GIL).
3. **Trigger primitives included** (global motion subtraction, spatial
   cluster, threshold + debounce + cooldown — not raw MVs).

## What we are claiming

The 0.2 release must back three headline numbers, each tied to a
reproducible bench in `benchmarks/results/`:

1. **Density:** at least N× more concurrent streams on a fixed CPU box
   than pixel-diff, where N is the headline ratio (target ≥ 3×).
2. **PTZ FP rate:** lower false-positive rate than pixel-diff on
   panning/tilt/zoom footage (CDnet 2014 PTZ category).
3. **Recall@event:** at least 90% recall on UCF-Crime anomaly events at
   a ≤ 5% sustained-CPU duty cycle on idle footage.

Anything below these means the wedge is not real and the project's
positioning has to change.

## P0 work items

Effort estimates assume single developer on M3 MacBook Air for code +
small benches, with one cloud GPU box rented for ~1 day for headline
numbers.

### P0.1 — Pixel-diff baseline runner

**Goal:** A reproducible pixel-diff motion detector implemented at the
same API surface as fovea-mv triggers, runnable on the same density
runner (`benchmarks/runners/density.py`) and the same accuracy bench.

**Why P0:** Without this number, "MV beats pixel-diff" cannot be claimed.

**Done when:**
- `benchmarks/baselines/pixel-diff/` contains a Python runner that
  decodes via PyAV/ffmpeg, computes frame-to-frame absolute diff, fires
  on threshold.
- Same `--source`, `--streams`, `--duration-s` interface as the fovea-mv
  density runner.
- Result file `benchmarks/results/<date>-pixeldiff-density.md` shows
  density curve on the same `cctv-sample` clip and hardware as the
  current smoke run.

**Effort:** 2–3 days.

### P0.2 — mv-extractor head-to-head

**Goal:** A throughput comparison against `mv-extractor` (the closest
existing library) on identical input.

**Why P0:** Defines the "vs mv-extractor" headline ratio.

**Done when:**
- `benchmarks/baselines/mv-extractor/` wraps `pip install mv-extractor`
  in the same runner shape.
- Result file shows N=1, 2, 4, 8 throughput on identical hardware and
  clip, with mv-extractor's GIL-bound single-stream cost called out.

**Effort:** 1–2 days.

### P0.3 — Multi-stream concurrency (Tokio)

**Goal:** A native Rust multi-stream scheduler so the comparison number
is not bottlenecked by the current Python multiprocessing runner.

**Why P0:** The current density bench is Python-multiprocessing wrapping
single-threaded Rust streams. To make a fair claim against pixel-diff
and mv-extractor at scale, fovea-mv needs a Tokio-based scheduler in
`fovea-mv-stream` that hosts N streams on a shared runtime.

**Done when:**
- `fovea-mv-stream` exposes a `MultiStream` type that takes N sources
  and yields events from any of them.
- Density runner has a Rust-native mode that uses `MultiStream` instead
  of forking Python workers.
- `benchmarks/results/<date>-density-tokio.md` updates the density curve
  with Rust-native numbers.

**Effort:** ~1 week.

### P0.4 — Global motion subtraction primitive

**Goal:** A `GlobalMotionSubtractTrigger` that, given a stream, estimates
the dominant per-frame MV (camera motion) and triggers only on residual
motion above a threshold.

**Why P0:** PTZ-friendliness is a primary differentiator; without this
primitive the PTZ FP claim is impossible.

**Done when:**
- New trigger type in `fovea-mv-core` with rustdoc + unit tests.
- Python binding in `fovea-mv-py`.
- `benchmarks/results/<date>-ptz-fprate.md` shows FP rate on CDnet 2014
  PTZ category vs pixel-diff baseline. Target: fovea-mv with global-motion
  subtraction << pixel-diff.

**Effort:** 3–5 days (including bench).

### P0.5 — Spatial cluster trigger

**Goal:** A `SpatialClusterTrigger` that fires only when motion vectors
cluster spatially (ignoring scattered noise from rain / wind / leaves).

**Why P0:** "Filter trivial environmental motion" is a key value-prop
claim; without a primitive that does it, the claim is unsupported.

**Done when:**
- New trigger type with cluster-size and density parameters.
- Python binding.
- Result file shows FP rate on CDnet 2014 dynamicBackground category vs
  pixel-diff and naive `MotionTrigger`.

**Effort:** 3–5 days.

### P0.6 — Hero example (`examples/mini_nvr.py`)

**Goal:** A ≤ 100-line example that takes N RTSP URLs, runs fovea-mv
triggers, and on each fire calls a YOLO (CoreML / ONNX) detector, then
optionally a VLM stub. Demonstrates the cascade in one file.

**Why P0:** "Library OSS without a hero example does not get adopted"
is the failure mode we are explicitly avoiding. README headline numbers
need a working example to anchor them.

**Done when:**
- `examples/mini_nvr.py` runs end-to-end on a single RTSP stream
  (mediamtx loop on the cctv-sample clip).
- README links it as the canonical "what does this do" entry point.
- 30-second screen capture / GIF showing it filtering out leaf motion
  and firing on a person walking through.

**Effort:** 1–2 days.

### P0.7 — Recall@event on UCF-Crime

**Goal:** A reproducible recall measurement on a public anomaly dataset.

**Why P0:** "≥ 90% recall on real CCTV events" is the third headline
number. UCF-Crime is the standard public source for this.

**Done when:**
- `benchmarks/datasets/ucf-crime/download.sh` fetches a 3–4 anomaly-class
  subset (~10 GB peak disk, deletable after run).
- `benchmarks/runners/recall.py` runs fovea-mv (with global-motion +
  spatial-cluster triggers) over the subset and reports recall vs the
  dataset's event annotations.
- Result file `benchmarks/results/<date>-ucf-crime-recall.md` reports
  recall + FP rate on idle stretches, with hardware line.

**Effort:** 2–3 days.

## Datasets — all public, no purchases

| Dataset | Use | Peak disk | License |
|---|---|---|---|
| CDnet 2014 (PTZ subset) | P0.4 PTZ FP rate | ~1–2 GB | research / non-commercial |
| CDnet 2014 (dynamicBackground subset) | P0.5 noise FP rate | ~1–2 GB | same |
| UCF-Crime (3–4 anomaly classes) | P0.7 recall | ~10 GB | research |
| Existing `cctv-sample` | P0.1, P0.2, P0.3 density | already present | already cleared |
| NYC DOT public RTSP cams (yt-dlp save) | optional 24h soak | ~3–5 GB | public stream, research use |

Workflow per CLAUDE.md "Reproducibility rules":
1. `download.sh` fetches the data into `benchmarks/datasets/<name>/data/`.
2. Bench runner consumes it, writes results to `benchmarks/results/`.
3. `make clean-data` deletes `benchmarks/datasets/*/data/` after the run.
4. `data/` is gitignored. Only `download.sh` and `README.md` are committed.

Peak disk at any one time: ~15 GB (during P0.7). MBA built-in storage
sufficient. No external SSD required.

## Hardware approach

- **Development + iteration:** M3 MacBook Air. All P0 code, small
  data benches, P0.1–P0.6 numbers.
- **Headline density numbers (P0.3, P0.7):** rent one cloud GPU box
  (Vast.ai / Lambda / RunPod) for ~1 day, ~$10–30. Justification:
  M3 thermal-throttles under sustained 100% load, no NVDEC, can't make
  "8 → 32 channels" claims credibly.
- Every result file declares hardware on a `Hardware:` line, per the
  evidence rule. MBA-derived numbers and cloud-derived numbers are
  reported separately, never mixed.

## Sequence

```
P0.1 pixel-diff baseline      ──┐
P0.2 mv-extractor head-to-head ─┤
                                ├─▶ P0.3 multi-stream concurrency
P0.4 global-motion subtraction ─┤    (uses comparison runners)
P0.5 spatial cluster            ┤
                                │
P0.6 hero example ◀────────────┘ (after P0.4 + P0.5 land)

P0.7 UCF-Crime recall — runs on top of P0.4 + P0.5 trigger composition
```

P0.1 and P0.2 are independent; do them in parallel sessions.
P0.4 and P0.5 are independent; same.
P0.6 and P0.7 depend on P0.4 + P0.5 being merged.

## What gets deferred

From 003:
- Tier 1: 24h+ soak — pull into P0.7 (long enough already), real IP
  camera purchase removed entirely (datasets cover it).
- Tier 1: HEVC RGB decode + PTS — keep as opportunistic fix, not
  gating P0.
- Tier 2: AV1 / VP9 — explicitly out of 0.2 scope.
- Tier 3: NVDEC / VideoToolbox GPU decode — only relevant if cloud bench
  shows CPU saturation as the bottleneck. Decide after P0.3.
- Tier 3: onset/offset trigger, confidence score — fold into P0.4 and
  P0.5 if the work calls for them, otherwise defer.
- Tier 4: CI + wheels + publish — slot in after the three headline
  numbers exist, before any 0.2 announcement.
- Phase 2 (`fovea-pick`) and Phase 3 (`fovea-stream`) — explicitly
  paused. Decide after 0.2 ships and we see adoption signal.

## Acceptance for closing 004

004 closes when all three headline numbers exist as committed
`benchmarks/results/` files and `examples/mini_nvr.py` runs end-to-end
on a clean clone. At that point:
- 003 moves to `docs/05.exec-plans/archive/003-roadmap.md`.
- 004 moves to `docs/05.exec-plans/archive/004-positioning.md`.
- README headline section gets the three numbers.
- Next plan (005) decides Phase 2/3 fate based on what we learned.

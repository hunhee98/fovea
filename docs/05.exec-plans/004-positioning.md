# 004 — fovea-trigger positioning + P0 wedge proof

Status: in progress — supersedes 003 sequence
Owner: @hunhee98
Subproject: `fovea-trigger`

## Why this plan exists

003 sequenced fovea-stream (Phase 3) as the next major piece on top of an
unproven Phase 1. That order is wrong. fovea-trigger has a working prototype
(001) and a multi-stream density smoke run (`benchmarks/results/2026-04-28-density-smoke.md`)
but no head-to-head numbers against pixel-diff or mv-extractor, no PTZ
handling, no hero example. Without those, "MV is better than the
alternatives" is an assertion, not a claim.

004 says: prove the fovea-trigger wedge first, then decide whether Phase 2/3
are the right next investment.

## Positioning statement

fovea-trigger is **a Rust-based multi-channel motion-vector trigger engine for
the decoder front-end of NVR/VMS systems.** Not a single-camera motion
detector. Not a research MV extractor.

Target users:
- OSS NVRs (Frigate, Viseron, AgentDVR) that want to host more channels
  per box.
- Integrators building custom VMS for sites with PTZ cameras.
- Anyone running a VLM cascade where pre-decode gating saves real money.

The wedge against existing tools:

| Existing tool | Layer | Gap fovea-trigger targets |
|---|---|---|
| ffmpeg `+export_mvs` | C API, raw MV access | no trigger primitives, no streaming integration, awkward to embed |
| mv-extractor (LukasBommes) | Python wrapper of ffmpeg | GIL-bound single-stream, file-oriented, no trigger logic |
| Frigate motion detector | pixel-diff in app | post-decode (decode cost paid up front), no global-motion handling, app-coupled |
| OpenCV background subtraction | pixel-domain | post-decode, no encoder-side info |

fovea-trigger differentiates on four axes simultaneously:
1. **Pre-decode gating** (bitstream → decision, no full decode in idle path).
2. **Multi-channel concurrent** (Rust + no-GIL).
3. **Trigger primitives included** (global motion subtraction, spatial
   cluster, threshold + debounce + cooldown — not raw MVs).
4. **Compressed-domain fusion** — MV alone misses lighting changes, new
   object appearance with no motion match, smoke / fire, and tampering
   bursts. fovea-trigger pairs MV with a second compressed-domain signal
   (intra-block ratio / residual energy, depending on tier) to cover
   those scenarios at bitstream cost. Prior art for the
   MV-plus-residual combination as a useful joint feature:
   [Wu et al. 2018 — arxiv:1712.00636] (CoViAR),
   [Shou et al. 2019 — arxiv:1901.03460] (DMC-Net). The novelty here is
   not the fusion idea but exposing it as a production-grade trigger
   primitive in OSS — academic implementations target CNN feature
   extraction, not thresholdable triggers, and Frigate / Viseron /
   mv-extractor stop at MV because ffmpeg's public API does too.

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
same API surface as fovea-trigger triggers, runnable on the same density
runner (`benchmarks/runners/density.py`) and the same accuracy bench.

**Why P0:** Without this number, "MV beats pixel-diff" cannot be claimed.

**Done when:**
- `benchmarks/baselines/pixel-diff/` contains a Python runner that
  decodes via PyAV/ffmpeg, computes frame-to-frame absolute diff, fires
  on threshold.
- Same `--source`, `--streams`, `--duration-s` interface as the fovea-trigger
  density runner.
- Result file `benchmarks/results/<date>-pixeldiff-density.md` shows
  density curve on the same `cctv-sample` clip and hardware as the
  current smoke run.

**Effort:** 2–3 days.

### P0.2 — Compressed-domain library capability matrix (was: mv-extractor head-to-head)

**Re-scoped 2026-04-29.** The original "throughput vs mv-extractor"
framing was wrong. Both projects call the same `libavcodec` API
(`+export_mvs`) under the hood, so single-stream raw-MV-extraction
throughput is approximately equal — there is no honest "N× faster"
claim to make at the H.264 level. fovea-trigger's actual advantage over
mv-extractor is in **what each library exposes**, not how fast each
one does the same thing.

**Goal (revised):** A side-by-side capability table covering the
features production NVR/VMS users actually care about, plus a
small-scale latency/throughput sanity check at N=1 to confirm the
"approximately equal" claim isn't off by a surprising factor.

**Why P0:** Without it, the README ends up making an unsupported
"vs mv-extractor" speed claim. With the capability table, the
comparison stays defensible — fovea-trigger covers HEVC, MODE_SKIP
ratio, CB-stats, glue-free RTSP, and ships trigger primitives;
mv-extractor returns raw MVs from H.264 only.

**Done when:**
- `docs/01.architecture/comparison-matrix.md` (or a section in
  `OVERVIEW.md`) lists fovea-trigger vs mv-extractor vs ffmpeg
  `+export_mvs` direct vs Frigate's pixel-diff motion module across
  these axes:
  - codecs supported (H.264 / HEVC / AV1 / VP9 explicit-error)
  - signals exposed (MV / intra ratio / skip ratio / global-motion
    correction / spatial cluster)
  - RTSP / live-source handling
  - trigger primitives ship out of the box
  - GIL / concurrency model
  - license
- A small `benchmarks/baselines/mv_extractor/` runner that wraps
  the canonical pip package on a Linux box (the package has no
  Apple Silicon wheels and the official Docker image is amd64-only,
  so this runs in cloud-bench tier — see Hardware approach).
- Result file `benchmarks/results/<date>-mv-extractor-N1-sanity.md`
  reports per-realtime-stream cost at N=1 on identical input,
  documenting that the wedge is *capability*, not raw-MV speed.

**Effort:** ~half a day for the capability table; ~1 hour cloud
bench when we rent a box for P0.7 anyway.

**Status:** capability table can land any time on MBA; the cloud
sanity run is bundled with the next cloud-tier session.

### P0.3 — Multi-stream concurrency (Tokio)

**Goal:** A native Rust multi-stream scheduler so the comparison number
is not bottlenecked by the current Python multiprocessing runner.

**Why P0:** The current density bench is Python-multiprocessing wrapping
single-threaded Rust streams. To make a fair claim against pixel-diff
and mv-extractor at scale, fovea-trigger needs a Tokio-based scheduler in
`fovea-trigger-stream` that hosts N streams on a shared runtime.

**Done when:**
- `fovea-trigger-stream` exposes a `MultiStream` type that takes N sources
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
- New trigger type in `fovea-trigger-core` with rustdoc + unit tests.
- Python binding in `fovea-trigger-py`.
- `benchmarks/results/<date>-ptz-fprate.md` shows FP rate on CDnet 2014
  PTZ category vs pixel-diff baseline. Target: fovea-trigger with global-motion
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

**Goal:** A ≤ 100-line example that takes N RTSP URLs, runs fovea-trigger
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

### P0.8 — Compressed-domain residual / intra-stats accessor (HEVC first)

**Goal:** Expose a second bitstream-level signal beyond MV — at minimum
the per-frame intra-block ratio in P/B slices, ideally graduating to
per-PB residual energy.

**Why P0:** MV alone is blind to several core surveillance scenarios:
sudden lighting change, new object appearance with no motion match
(door opens, package dropped), smoke / fire (no clear motion field),
tampering bursts. An "intra-coded block in a P-slice" is the encoder
literally giving up on motion prediction for that region — a direct
signal for those scenarios. CoViAR / DMC-Net validate the academic side
of MV + residual fusion; the OSS gap is that no production library
exposes either as a trigger primitive (ffmpeg public API stops at MV).

**Tier ladder (we ship the cheapest tier that passes the recall bar
for the targeted scenarios; only graduate if the cheaper tier loses):**

| Tier | Signal | Decoder change | Per-frame cost vs MV-only |
|---|---|---|---|
| **T1** | CB-level intra-block ratio (already stored in `cb_info.PredMode`) | none | ~0 |
| T2 | Per-frame CBF density (TU-level coded-block flags) | hook into `decode_TU` | low |
| T3 | Per-PB residual L1 magnitude | hook into inverse-quant path | mid |

T1 first because it requires zero changes to the decoder hot path.

**Done when (T1, this plan):**
- `de265_internals.h` exposes `de265_internals_get_CB_stats(image, *out)`
  returning `{total_cells, intra_cells, inter_cells, skip_cells, *_pixels,
  slice_type_first}`. **Status: scaffolded 2026-04-29 (commit pending).**
- Rust FFI binding in `crates/fovea-trigger-core` exposes a
  `Frame::cb_stats()` method.
- New trigger `IntraRatioTrigger { threshold, slice_types }` that fires
  when intra-pixel-ratio in P/B slices exceeds the threshold.
- Python binding in `fovea-trigger-py`.
- Result file `benchmarks/results/<date>-intra-ratio-pilot.md` runs the
  trigger over the cctv-sample clip + a synthetic "lights off / lights
  on" clip + a CDnet "fall" clip and shows the trigger fires on the
  events MV alone misses, with the per-frame cost vs MV-only also
  reported.

**Effort (T1):** ~1 week including the Rust binding and pilot bench.

**Out of scope (this plan):** H.264 residual extraction. ffmpeg's public
API does not expose either MVs or residuals at the level we need; the
H.264 path will require either a libavcodec patch or an alternative
parser, and is non-trivial. Punt to a follow-up plan once T1's signal
quality justifies the investment.

### P0.9 — Compressed-domain fusion trigger

**Goal:** A `FusionTrigger` that combines `MotionTrigger` (existing) and
`IntraRatioTrigger` (P0.8) into a single primitive with documented
decision semantics.

**Why P0:** P0.8 by itself is one new signal; the headline claim is
that **MV + intra together** beats MV alone and beats pixel-diff alone
across the scenario matrix. Without a fusion primitive there is no
single trigger to point users at.

**Done when:**
- `FusionTrigger` exposes a 2D-thresholded fire condition (one threshold
  per signal, plus a combined-OR / combined-AND / weighted-sum mode).
- Documentation enumerates the four-quadrant semantic table:

  | MV | Intra | Interpretation |
  |---|---|---|
  | low | low | idle (suppress) |
  | low | high | new content / lighting / smoke (fire) |
  | high | low | clean motion / pan (delegate to global-motion subtractor) |
  | high | high | sudden event / scene change (fire) |

- Result file extends P0.7 (UCF-Crime recall) and P0.4 (PTZ FP) numbers
  with `FusionTrigger` rows alongside `MotionTrigger`-only and
  `pixel-diff`-only baselines.

**Effort:** 3–4 days after P0.8 lands.

### P0.10 — Measure cbf marginal value (HEVC) and motion-only baseline (H.264)

**Re-scoped before any H.264 cbp work.** We have HEVC `cbf_density`
shipped (T2 in P0.8). Before investing months in extracting an
equivalent signal from H.264 — which has no public-API path and would
need either a libavcodec fork or a custom syntax parser — we need
direct evidence that cbf actually moves the trigger needle. Then,
and only then, does H.264 cbp become a justified investment.

**Goal:** Measure how much CBF density reduces false-positive rate
on the trigger task itself (not on a CNN classification task — the
literature's evidence is mostly action-recognition CNNs, which is
not what we ship).

**The actual question:** Does adding `cbf_density` as a third axis
to the FusionTrigger meaningfully reduce false-positive rate on
challenging surveillance inputs (dynamic background, PTZ panning),
relative to the same trigger using only motion + intra signals?

**Done when:**
- `benchmarks/datasets/cdnet-2014/download.sh` pulls the
  `dynamicBackground`, `PTZ`, and `intermittentObjectMotion`
  categories (PNG sequences, ~3 GB total).
- A small re-encode step turns each category's PNG sequence into
  matched H.264 + HEVC mp4s with controlled encoder settings
  (libx264/libx265, default preset, fixed CRF), so the comparison is
  encoder-controlled rather than encoder-confounded.
- A new runner `benchmarks/runners/cbf_marginal.py` runs each
  source through three trigger configurations:
  - `H.264 motion-only` (current path; cbf unavailable)
  - `HEVC motion-only` (FusionTrigger with cbf disabled)
  - `HEVC motion + cbf` (FusionTrigger with cbf enabled)
  and reports per-category TP / FP / per-hour FP rate against the
  CDnet ground-truth masks.
- Result file `benchmarks/results/<date>-cbf-marginal.md` lays out
  the table and draws the explicit decision:
  - "cbf reduces FP by ≥ 30% on at least one category" → H.264 cbp
    is worth the investment; proceed to P0.11.
  - "cbf reduces FP by < 10%" → cbf marginal; H.264 cbp not worth
    the maintenance burden; H.264 stays motion-only and we ship 0.3
    that way.
  - In-between → re-evaluate with a second dataset.

**Hardware caveat:** Re-encoding CDnet PNGs introduces our own
encoder choices (libx264 / libx265 default), so the absolute FP
numbers are not portable to a real Hikvision / Axis stream — but
the *relative* "cbf on vs off" delta is the signal we care about and
that delta is robust to encoder choice.

**Effort:** 1–2 days for download + re-encode + runner; another
day for analysis and result write-up.

### P0.11 — H.264 cbp accessor (gated on P0.10)

**Status:** Conditional. Only starts if P0.10 says cbf marginal value
is real (≥ 30% FP reduction on at least one challenging category).

**Three candidate paths**, evaluated in 002 P0.2 and the surrounding
discussion:
1. libavcodec fork with `+export_cbp` flag — initial 1–2 months,
   *ongoing* maintenance burden every ffmpeg release. Strongest
   integration but largest follow cost.
2. openh264 (Cisco, BSD-2) fork. Smaller codebase than ffmpeg, less
   active mainline, lighter follow burden, similar initial work.
3. Hybrid syntax parser — keep ffmpeg for demux, write a custom
   H.264 syntax-only reader (mb_layer + cbp + CABAC subset, no IDCT,
   no MC, no loop filter). Larger initial investment (~3 months)
   but zero ongoing maintenance — H.264 spec is frozen.

The 0.3 cycle picks one of these based on: the measured cbf delta
from P0.10, whether mainline upstream interest exists for option 1,
and how much of the H.264 syntax we actually need (cbp + mb_type +
mb_skip is a small fraction of the spec).

Reference path: port openh264's CABAC reader (BSD-2) into Rust as
the entropy decoder, validate bit-exact against ffmpeg-internal
ground-truth dumped from `cctv-sample`, then write only the
mb_layer slice subset that fovea-trigger consumes.

**Effort:** 2–4 months depending on path chosen.

### P0.7 — Recall@event on UCF-Crime

**Goal:** A reproducible recall measurement on a public anomaly dataset.

**Why P0:** "≥ 90% recall on real CCTV events" is the third headline
number. UCF-Crime is the standard public source for this.

**Done when:**
- `benchmarks/datasets/ucf-crime/download.sh` fetches a 3–4 anomaly-class
  subset (~10 GB peak disk, deletable after run).
- `benchmarks/runners/recall.py` runs fovea-trigger (with global-motion +
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
| CDnet 2014 (intermittentObjectMotion + the two above) | **P0.10 cbf marginal** | ~3 GB total | same |
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
P0.1 pixel-diff baseline       ──┐
P0.2 mv-extractor head-to-head ──┤
                                 ├─▶ P0.3 multi-stream concurrency
P0.4 global-motion subtraction ──┤   (uses comparison runners)
P0.5 spatial cluster           ──┤
P0.8 intra-ratio accessor T1   ──┘   (HEVC first; libde265 internals)
                                 │
P0.9 fusion trigger ◀───────────┘   (needs P0.4 + P0.5 + P0.8)
P0.6 hero example   ◀───────────┘   (needs P0.4 + P0.5 + P0.9)
P0.7 UCF-Crime recall — runs on top of P0.9 fusion trigger
```

P0.1 and P0.2 are independent; do them in parallel sessions.
P0.4, P0.5, P0.8 are independent; same.
P0.9 depends on P0.4 + P0.5 + P0.8.
P0.6 and P0.7 depend on P0.9.

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

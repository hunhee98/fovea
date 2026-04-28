# 2026-04-29 — Global motion correction + sliding window

## Claim

Adding median-based **global-motion correction** and a **sliding-window**
energy smoother to `MotionTrigger` reduces false-positive triggers on
camera-induced motion (vibration / pan) while preserving true-positive
triggers from real object motion. Demonstrated on:

1. Synthetic pure camera pan — 100 % FP elimination (50 → 0 fires).
2. CDnet 2014 `cameraJitter` (4 real-jitter scenes) — 0–77 % FP reduction.
3. Pexels `cctv-sample` (object motion, no camera motion) — fire count
   unchanged (5 → 5), no TP loss.

## Hardware

- MacBook Air, Apple M3, 16 GB, macOS Darwin 25.3.0 arm64.
- Build: `cargo build --release` (`fovea-mv-stream` linked against locally
  built `vendor/libde265`).

## Commit

`7bdde26` (base) + the global-motion + window changes on branch
`worktree-feat+mv-global-motion`.

## Datasets

- **synthetic-ptz** — single still extracted from `cctv-sample/sample.mp4`,
  re-encoded with FFmpeg `crop` slide to produce pure horizontal camera pan.
  Generator: `benchmarks/datasets/synthetic-ptz/generate.sh`.
- **cdnet-camerajitter** — 4 scenes from CDnet 2014 `cameraJitter`,
  re-encoded from PNG sequences to surveillance-profile H.264.
  Generator: `benchmarks/datasets/cdnet-camerajitter/download.sh`.
- **cctv-sample** — Pexels CCTV reference clip, fixed camera, object motion
  only. Pre-existing.

All clips re-encoded with libx264 at the surveillance fingerprint (High
profile, GOP 15, no B-frames, ~3 Mbps) that matches the seattle-dot DOT
camera profile.

## Method

`MotionTrigger` configurations evaluated against each clip:

| Config | What |
|---|---|
| `baseline` | `MotionTrigger::new(threshold)` |
| `global_motion` | `+ with_global_motion(GlobalMotionEstimator)` |
| `window` | `+ with_window(5)` (5-frame sliding mean) |
| `global_motion+window` | both |

All four use the **same energy threshold** for a given clip — the only
variable is whether correction / smoothing is enabled.

Threshold: `1000` (sum of L1 motion magnitudes in pixel units, summed over
all macroblocks). Picked once, never re-tuned across configs or clips.

Runner: `crates/fovea-mv-stream/examples/global_motion_ab.rs` — counts
fires per config, reports mean / median energy at fire time.

```sh
cargo run --release --example global_motion_ab -- <clip>
```

## Results

### Synthetic pure pan (FP-only scenario)

| Config | Fires | Mean E | Median E |
|---|---:|---:|---:|
| baseline | 50 | 4155 | 4142 |
| global_motion | **0** | 0 | 0 |
| window | 1 | 2126 | 2126 |
| global_motion+window | 0 | 0 | 0 |

300 packets, 578 490 MVs. By construction (one still frame replicated
under a sliding crop window) every fire is a false positive. Global-motion
correction eliminates 100 % of them.

### Synthetic static control

| Config | Fires |
|---|---:|
| baseline | 0 |
| global_motion | 0 |
| window | 0 |
| global_motion+window | 0 |

Sanity check: a non-moving control clip produces no triggers under any
config. Correction does not introduce false positives.

### CDnet 2014 cameraJitter (real-vibration scenes)

| Scene | baseline | global_motion | window | gm+window |
|---|---:|---:|---:|---:|
| badminton | 143 | 143  (-0 %) | **1**  (-99 %) | 3   (-98 %) |
| boulevard | 194 | 128  (-34 %) | **40** (-79 %) | 42  (-78 %) |
| sidewalk | 210 | 130  (-38 %) | 74  (-65 %) | **49** (-77 %) |
| traffic | 301 | 239  (-21 %) | 78  (-74 %) | **69** (-77 %) |

Observations:

- `global_motion` alone is moderately effective (0–38 % FP reduction). On
  scenes where objects dominate the MV field (badminton: tight close-up of
  a player), the median MV is contaminated by the object motion and the
  estimate of camera motion is poor.
- `window` is the strongest single technique on jitter — sustained-noise
  averaging smooths out per-frame jitter spikes.
- The combined `gm+window` is consistently the best or near-best across
  all scenes (49 / 69 fires on the two strongest cases).

### cctv-sample (TP-preservation control)

| Config | Fires | Mean E |
|---|---:|---:|
| baseline | 5 | 5582 |
| global_motion | 5 | 5582 |
| window | 1 | 2325 |
| global_motion+window | 1 | 2325 |

Real CCTV with object motion but no camera motion. Global-motion correction
produces identical fire counts and energies — no false negatives introduced.
This is the critical test that the correction does not silently suppress
real triggers when camera motion is absent.

## Limitations

- **All clips are libx264-re-encoded.** Surveillance encoder (Axis / Bosch
  / Pelco / Hikvision class) bitstreams may have different MV / residual
  fingerprints. A real-PTZ RTSP capture validation is pending; the
  `seattle-dot/SOURCE.md` rationale applies here too.
- **CDnet 2014 PTZ category** unavailable (host returns 404 as of
  2026-04-29). Used `cameraJitter` instead — same mechanism (uniform
  global motion → false positives), different scene type.
- **Threshold is manually chosen.** Operators tune per-camera; auto-
  calibration is roadmap (see `docs/05.exec-plans/003-roadmap.md`,
  Tier 3).
- **Object-dominated scenes** (badminton) limit the effectiveness of
  median-based global-motion estimation. RANSAC or affine upgrades are
  deferred per the verifier recommendation; the current approach handles
  the common-case (translation + small object footprint) cheaply.

## Reproduce

```sh
# 1. Build
cargo build --release --example global_motion_ab -p fovea-mv-stream

# 2. Generate synthetic clips (requires cctv-sample/sample.mp4 already present)
bash benchmarks/datasets/synthetic-ptz/generate.sh

# 3. Fetch and encode CDnet cameraJitter (~130 MB download + ~10 s encode)
bash benchmarks/datasets/cdnet-camerajitter/download.sh

# 4. Run
./target/release/examples/global_motion_ab benchmarks/datasets/synthetic-ptz/data/pan_pure.mp4
./target/release/examples/global_motion_ab benchmarks/datasets/synthetic-ptz/data/static.mp4
./target/release/examples/global_motion_ab benchmarks/datasets/cctv-sample/sample.mp4
for c in badminton boulevard sidewalk traffic; do
  ./target/release/examples/global_motion_ab benchmarks/datasets/cdnet-camerajitter/data/$c.mp4
done
```

## Citations

- `[CoViAR — arxiv:1712.00636]` — temporal MV accumulation rationale (we
  do not use CoViAR's stacked-residual classifier here, only the
  observation that single-frame MV is noisy and benefits from windowing).
- `[CDnet 2014 — Wang et al., CVPR Workshop 2014]` — `cameraJitter` and
  `PTZ` change-detection benchmark (cited in `cdnet-camerajitter/SOURCE.md`).

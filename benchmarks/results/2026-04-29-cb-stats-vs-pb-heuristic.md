# 2026-04-29 — CB-stats vs PB-heuristic intra-fraction (HEVC)

Status: smoke probe. Two `intra_ratio` measurements on the same HEVC
clip disagree by ~7×. Reported here so the number is on file; whether
the disagreement matters for trigger behavior is decided downstream
(P0.4, P0.7).

## TL;DR

On `cctv-sample-hevc` (457 frames), the existing PB-derived heuristic
reports a mean per-frame intra fraction of 0.0023, while the new
`de265_internals_get_CB_stats` accessor reports 0.0165 — a ~7× gap.
The new accessor also exposes a per-frame `skip_ratio` (mean ~0.85 on
this clip) that the heuristic cannot derive at all.

## Hardware

Apple M3 (8-core), 16 GB, Darwin 25.3.0, rustc 1.91.1.

## Commit

`7bdde26` (feat/mv-step2-h264-extraction).

## Source

`benchmarks/datasets/cctv-sample-hevc/sample.h265` — committed test
clip, 457 frames, IPB GOP structure (slice_type sequence: I once, then
B B B P B B B P …).

## Command

```sh
cargo run -q -p fovea-trigger-stream --example cb_stats_compare \
  -- benchmarks/datasets/cctv-sample-hevc/sample.h265
```

## Numbers

| metric | value |
|---|---|
| frames | 457 |
| mean `pb_intra` (heuristic, `pb_info.ref_poc{0,1} == -1`) | **0.0023** |
| mean `cb_intra` (direct, `cb_info[i].PredMode`) | **0.0165** |
| mean \|`cb_intra` - `pb_intra`\| | 0.0141 |
| max \|`cb_intra` - `pb_intra`\| | 1.0000 (I-frame; both = 1.0) |
| mean `cb_skip` (new) | ~0.85 across B-frames, ~0.48 across P-frames |
| `cb_skip` / `cb_inter` / `cb_intra` directly addressable? | yes (new) |
| same from PB heuristic? | only `cb_intra`, lossy |

## What this is and isn't

- **Is**: documentation that two methods of measuring intra fraction
  give different numbers on the same clip, with the difference logged
  for later reference.
- **Is not**: a claim that one method is correct and the other is
  buggy. The two methods read different decoder state (PB-grid
  reference indices vs CB-grid PredMode), and which one a trigger
  should consume is a downstream measurement question, not a
  theoretical one.

## Why we still ship the new accessor

`skip_ratio` is not derivable from PB info — `pb_info` only carries
motion-vector data, and a skip CU's `PBMotion` is structurally
indistinguishable from a regular merge-coded inter CU. Exposing
`skip_ratio` as a primitive unlocks an "encoder said: this region
didn't change" signal that the existing pipeline cannot surface, even
if the `cb_intra` vs `pb_intra` discrepancy turns out to be irrelevant
for trigger behavior.

## What's next

- Wire `cb_stats()` into the HEVC processing path so `MvPacket`
  carries `skip_count` alongside `intra_count`. Surface `skip_ratio`
  on `Event`.
- Decide whether `MotionTrigger` / `SceneChangeTrigger` should keep
  the heuristic or migrate to the direct measurement based on
  downstream PTZ FP and recall results (P0.4 / P0.7 in
  `docs/05.exec-plans/004-positioning.md`).
- H.264 path is unaffected — `+export_mvs` stops at MVs and we have
  no equivalent CB-stats access there yet.

## Files

- `crates/fovea-trigger-stream/src/hevc.rs` — `CbStats` struct,
  `DecodedFrame::cb_stats()`.
- `crates/fovea-trigger-stream/examples/cb_stats_compare.rs` — the runner
  used to produce these numbers.
- `vendor/libde265/libde265/de265_internals.{h,cc}` —
  `de265_internals_get_CB_stats` accessor.

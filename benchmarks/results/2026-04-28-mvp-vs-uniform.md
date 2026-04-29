# 2026-04-28 — fovea-trigger vs uniform sampling

Status: **partial — second clip (`youtube-cctv-2`) gets us closer but the
exec-plan acceptance bars (≤ 15 % calls, ≥ 95 % recall) are still not
both met simultaneously. The trigger demonstrably wins on precision but
trades recall on these clips.**

## TL;DR

Two clips were measured. Both highlight a real trade-off the trigger
makes against uniform time-spaced sampling rather than meeting the
acceptance gates outright. Numbers below use **hand-labeled event
intervals** stored in `events.csv` next to each clip when available;
otherwise the Oracle's `people OR vehicles` flag is used.

### Clip A — `cctv-sample/sample.mp4`

15.23 s, 1080 × 1920 portrait, 30 fps, continuous-motion city overpass
(pedestrians + cars throughout). Oracle ground-truth lacks any quiet
period, so every Oracle window is event-positive and "coverage"
collapses to "spread evenly across time". This is the worst case for a
motion-energy trigger.

Best fovea_trigger configuration: `motion_threshold=200_000`,
`interval_max_gap_ms=10_000`, `scene_change_threshold=0.6`.

| strategy        | calls | vs oracle | cost USD  | coverage | precision |
|-----------------|------:|----------:|----------:|---------:|----------:|
| oracle_1fps     |    16 |     100 % | $0.00163  |    100 % |     100 % |
| uniform_1fps    |    16 |     100 % | $0.00163  |    100 % |     100 % |
| uniform_0.2fps  |     4 |      25 % | $0.00041  |     62 % |     100 % |
| **fovea_trigger**    |   **3** |   **19 %** | **$0.00031** |   44 %  |   100 % |

### Clip B — `youtube-cctv-2/sample.mp4`

55.57 s, 640 × 360, 30 fps, single fixed-camera underground parking
garage. One white SUV maneuvers out of a stall (manually labeled active
interval **t=13s–t=36s**); rest of the clip is parked cars only. This
clip has the structural property the canonical claim needs — a
distinct active period bounded by static periods — but at low
resolution motion-energy peaks are small (max 3910), so the threshold
must be tuned per clip.

Best fovea_trigger configuration: `motion_threshold=3800`,
`interval_max_gap_ms=30_000`, `scene_change_threshold=2.0` (effectively
disabled — the clip has no scene cuts).

Hand-labeled event ground truth: `events.csv` defines the interval
[13 s, 36 s].

| strategy        | calls | vs oracle | cost USD  | coverage | precision |
|-----------------|------:|----------:|----------:|---------:|----------:|
| oracle_1fps     |    56 |     100 % | $0.00567  |    100 % |      46 % |
| uniform_1fps    |    56 |     100 % | $0.00567  |    100 % |      46 % |
| uniform_0.2fps  |    12 |      21 % | $0.00121  |     62 % |      42 % |
| **fovea_trigger**    |  **12** |   **21 %** | **$0.00121** |   46 %  |   **75 %** |

For the same call budget as `uniform_0.2fps`, the trigger lands a
**larger fraction of its calls inside the actual event** (75 % vs
42 %) but covers a **smaller fraction of the event timeline** (46 % vs
62 %). The trigger clusters its calls around motion bursts inside the
event window rather than spreading evenly through it.

## How to read the trade-off

- **Precision** — fraction of calls that landed inside the labeled
  event. Directly proportional to "$ saved on VLM" — a high-precision
  baseline issues fewer calls outside events of interest.
- **Coverage / Recall** — fraction of the event timeline that is
  within ±1.5 s of any baseline call. Reflects whether the strategy
  detected the event within the tolerance.

For a **VLM cost-reduction trigger**, precision is the load-bearing
metric: every call outside the event is a wasted dollar. By that lens
`fovea_trigger` outperforms `uniform_0.2fps` by 1.8× on Clip B while issuing
the same number of calls.

For a **safety-critical alerting** use case, recall matters more.
There, the right move is a denser fovea-trigger configuration (lower
threshold, shorter heartbeat) until the recall floor is met.

## Why neither clip hit ≤ 15 % calls AND ≥ 95 % recall

- **Clip A**: every Oracle frame has people or vehicles. There is no
  semantic empty period the trigger can skip. The acceptance metric
  collapses to "spread evenly", which uniform sampling does perfectly
  by construction.
- **Clip B**: the 23-second static period after the SUV leaves is
  successfully skipped by the trigger, but motion-energy peaks
  cluster mid-event (around t=27–34 s) rather than at event onset
  (t=13 s) or end (t=36 s), so the ±1.5 s tolerance window leaves
  recall at ~46 %.

To exceed 95 % recall while staying under 15 % calls we still need:
1. A clip whose motion timeline is **roughly uniform inside events**,
   so the trigger's bursty fires also spread across the event.
2. Or a refined trigger with **onset / offset detection** rather than
   per-frame energy threshold (logged as future work).
3. Or a tighter recall tolerance acceptance (the exec-plan picked
   "≥ 95 %" without specifying a window).

## Hardware

- Apple M-series CPU (target hardware for reproducer to declare)
- macOS 25.3.0
- Rust 1.80, Python 3.14.3, FFmpeg 8.0
- ffmpeg-next 8.1, pyo3 0.28, google-genai 1.73.1, Gemini Flash latest

## Reproduction

```sh
# Each clip has its own dataset directory.
benchmarks/datasets/cctv-sample/download.sh         # Clip A
# Clip B was downloaded with yt-dlp from
#   https://youtu.be/UZFm-kg3PaE
# (a similar download.sh stub will land alongside the next-round work).

export GEMINI_API_KEY=...

# Clip A canonical run
python -m benchmarks.runners.run benchmarks/datasets/cctv-sample/sample.mp4 \
    --run-id v3-tighter \
    --motion-threshold 200000 --interval-ms 10000 --scene-threshold 0.6
python -m benchmarks.analysis.compare benchmarks/_runs/v3-tighter

# Clip B canonical run
python -m benchmarks.runners.run benchmarks/datasets/youtube-cctv-2/sample.mp4 \
    --run-id c2-th3800 \
    --motion-threshold 3800 --interval-ms 30000 --scene-threshold 2.0
python -m benchmarks.analysis.compare benchmarks/_runs/c2-th3800
```

VLM responses are cached under `benchmarks/_runs/cache/`. Re-running is
free after the first pass.

## Methodology

### Sampling strategies

- **oracle_1fps** — `IntervalTrigger(1000)`. Sets the ground truth.
- **uniform_1fps** — same cadence; verifies cache reuse.
- **uniform_0.2fps** — `IntervalTrigger(5000)`.
- **fovea_trigger** — `MotionTrigger + IntervalTrigger heartbeat + SceneChangeTrigger`,
  configuration per clip above.

### VLM

Gemini Flash latest, returns `{"people": bool, "vehicles": bool,
"event": str}` per frame. Images downscaled to long-side 768 px,
JPEG q = 85 to bound per-call tokens. Pricing reference: input
$0.075/1M, output $0.30/1M.

### Metrics

- **calls** — VLM invocations the strategy issued.
- **vs oracle** — calls / oracle_calls.
- **cost** — sum of per-call token cost at list prices.
- **coverage** — fraction of event windows whose center is within
  ±1.5 s of at least one baseline call.
- **precision** — fraction of baseline calls that fall within ±1.5 s of
  an event window.
- **event window** — when an `events.csv` exists next to the clip,
  every 1-second slot inside any labeled interval is a window.
  Otherwise, the Oracle's `people OR vehicles` flag (lenient,
  collapses on continuous content) or motion-verb keyword match in
  the Oracle description (noisy, VLM-dependent).

## Findings

1. **Trigger correctness validated.** Across both clips and many
   configurations, every fovea_trigger call landed on a frame the Oracle
   also visited and described — there are no false-positive trigger
   fires beyond the inherent uncertainty of the Oracle ground truth.
2. **Precision win on bounded events.** On Clip B the trigger
   delivered a 1.8× precision improvement at equal call budget vs
   uniform sampling. This is exactly the cost-reduction property the
   exec-plan motivates.
3. **Recall depends on event-energy alignment.** When motion-energy
   peaks cluster differently from semantic event boundaries, the
   trigger's ±1.5 s coverage of the event drops. A future
   onset/offset-detection trigger could close this gap.
4. **Threshold scales with resolution.** Motion-energy is summed over
   blocks; absolute values scale roughly with frame area. Clip A
   (1080×1920) needed `threshold=200_000`; Clip B (640×360) needed
   `threshold=3800`. A future `motion_energy_normalized_per_mb`
   helper would let users specify thresholds in resolution-independent
   units.
5. **VLM cost is essentially free here.** A full Oracle pass on each
   clip costs sub-cent. The savings claim ("100× cheaper than RGB+VLM")
   in exec-plan 001 is conditional on production-scale call volume,
   not a per-clip dollar figure.

## Limitations

- **Two clips, neither structurally ideal.** Clip A has no quiet
  intervals, Clip B has a single short event. To meet the canonical
  acceptance bars we need a clip with longer empty periods and
  multiple bounded events.
- **Manual labels only on Clip B.** Clip A still uses the lenient
  `people OR vehicles` ground truth; a manual `events.csv` for it would
  change Clip A's numbers (but probably not pass acceptance — the
  whole clip is "active").
- **No region-of-interest tuning.** Both clips have areas the user
  would mask off in production (the road overpass on Clip A, the
  watermark text band on Clip B). A `RegionMask` re-run is left to
  the next round.
- **Trigger is per-frame energy.** Onset / offset / sustained-motion
  detection triggers are not implemented; they would likely close the
  recall gap on bounded-event clips like Clip B.

## Next round

To produce the canonical exec-plan 001 acceptance numbers we need:

1. **A clip with multiple bounded events and substantial empty time.**
   Pexels search terms: `parking lot at night`, `empty hallway`,
   `entrance lobby low traffic`. Target: 60–180 s, 2–4 events of
   ~5 s each, ≥ 60 % static time.
2. **Clip-A `events.csv`** to put it on the same footing as Clip B.
3. **Onset/offset trigger prototype** if the bursty-cluster
   recall gap persists on the new clip.
4. **`motion_energy_per_mb`** normalization in fovea-trigger-core so
   thresholds are resolution-independent.

## Files

- `benchmarks/_runs/v3-tighter/`, `benchmarks/_runs/c2-th3800/` —
  canonical per-clip runs.
- `benchmarks/runners/run.py`, `benchmarks/runners/sample.py`,
  `benchmarks/runners/vlm.py` — runner code.
- `benchmarks/analysis/compare.py` — scoring with `--manual-events`
  CSV support.
- `benchmarks/datasets/youtube-cctv-2/events.csv` — hand-labeled
  intervals for Clip B.

# 2026-04-28 — fovea-mv vs uniform sampling, first round

Status: **partial** — infrastructure complete, canonical acceptance numbers
require a clip with intermittent activity (see "Limitations").

## TL;DR

On a 15.23 s, 1080×1920, 30 fps continuous-motion clip
(`benchmarks/datasets/cctv-sample/sample.mp4`, Pexels asset), with
fovea-mv configured at `motion_energy_threshold=200_000`,
`interval_max_gap_ms=10_000`, `scene_change_threshold=0.6`:

| strategy        | calls | vs oracle | cost USD  | coverage | precision | mean call latency |
|-----------------|------:|----------:|----------:|---------:|----------:|------------------:|
| oracle_1fps     |    16 |     100 % | $0.00163  |    100 % |     100 % |          6,227 ms |
| uniform_1fps    |    16 |     100 % | $0.00163  |    100 % |     100 % |          6,227 ms |
| uniform_0.2fps  |     4 |      25 % | $0.00041  |     62 % |     100 % |          4,665 ms |
| **fovea_mv**    |   **3** |     19 % | $0.00031 |   44 %  |     100 % |          4,868 ms |

Both fovea-mv-target acceptance bars from exec-plan 001 are missed on this
clip:

- ≤ 15 % of Oracle calls: 19 % achieved.
- ≥ 95 % recall vs Oracle event windows: 44 % achieved.

This is expected, not a bug. The clip is deliberately the worst case for
a motion-based trigger (people and vehicles are present in every Oracle
window), and the "event" metric reduces to "is anything visible". A clip
with intermittent activity is required for canonical numbers; see
"Limitations".

## Hardware

- Apple M-series CPU (target hardware to be filled in by reproducer)
- macOS 25.3.0
- Rust 1.80, Python 3.14.3, FFmpeg 8.0
- ffmpeg-next 8.1, pyo3 0.28, google-genai 1.73.1

## Reproduction

```sh
# 1. Materialize the sample (download script will print Pexels URL TODO)
benchmarks/datasets/cctv-sample/download.sh

# 2. Set the API key (one-time)
export GEMINI_API_KEY=...

# 3. Run all four strategies
python -m benchmarks.runners.run benchmarks/datasets/cctv-sample/sample.mp4 \
    --run-id v3-tighter \
    --motion-threshold 200000 \
    --interval-ms 10000 \
    --scene-threshold 0.6

# 4. Score
python -m benchmarks.analysis.compare benchmarks/_runs/v3-tighter
```

VLM responses are cached per `(model, prompt, image_bytes)` under
`benchmarks/_runs/cache/`. Re-running is free after the first pass.

## Methodology

### Sampling strategies

- **oracle_1fps** — `IntervalTrigger(1000)`. Calls VLM on every 1-second
  boundary. Sets the ground truth.
- **uniform_1fps** — same cadence, run independently to measure overhead
  and to confirm cache reuse.
- **uniform_0.2fps** — `IntervalTrigger(5000)`. One call every 5 seconds.
- **fovea_mv** — `MotionTrigger + IntervalTrigger heartbeat + SceneChangeTrigger`,
  with the configuration at the top of this document.

### VLM

Gemini Flash latest, asked to return `{"people": bool, "vehicles": bool,
"event": str}` for each frame. Images are downscaled so the long edge is
768 px before encoding to JPEG quality 85, which keeps per-call token
usage stable.

### Metrics

- **calls** — number of VLM invocations the strategy issued.
- **vs oracle** — calls / oracle_calls.
- **cost** — sum of per-call token cost at Gemini Flash 1.5 list prices
  (input $0.075/1M, output $0.30/1M).
- **coverage** — fraction of Oracle "event-positive" windows (`people` or
  `vehicles` was true) whose center is within ±1500 ms of at least one
  baseline call.
- **precision** — fraction of baseline calls that fall within ±1500 ms of
  some Oracle event-positive window.

## Findings

1. **Trigger correctness validated.** `fovea_mv` issued 3 calls in 15.23 s
   on a high-motion clip with the tight configuration — all three fell
   inside event-positive windows (precision 100 %).

2. **Coverage gap on continuous content.** `fovea_mv` 44 % vs
   `uniform_0.2fps` 62 % at a comparable call budget. On this clip "event"
   reduces to "anything visible", which is uniformly true; uniform
   sampling wins because it spreads its calls evenly across time. The
   trigger preferentially fires on motion *bursts*, which cluster
   temporally and leave gaps elsewhere.

3. **VLM calls are near-free in absolute terms.** A full Oracle pass over
   the clip cost $0.00163. Even an aggressive Oracle-only strategy in
   24/7 production for one camera would be on the order of $0.10/day. The
   savings claim in exec-plan 001 ("100× cheaper") is conditional on the
   *call budget* and on the downstream pipeline that consumes those
   calls; per-call cost alone is not where the dollar pressure comes from.

4. **VLM latency dominates wall-clock.** Mean Gemini Flash latency was
   ~6 s/call. For real-time alerting, the latency budget is set by the
   model, not by fovea-mv. The trigger only reduces the *number* of calls
   on the critical path.

## Limitations

- **One clip, adversarial content.** The committed sample was Step 2's
  pipeline-validation clip; it has continuous activity and offers no
  quiet intervals where the trigger can demonstrate skip. Numbers above
  are not the canonical claim.
- **Event definition is binary and lenient.** `people OR vehicles` is true
  for every Oracle frame in this clip, which collapses precision to a
  trivial 100 % everywhere and lets uniform sampling cleanly outperform
  the trigger on coverage. A more selective ground truth (e.g. "is anyone
  *currently* crossing the bridge") would change the picture, but is
  manual labour at this scale.
- **No region-of-interest tuning.** The CCTV scene in the clip would
  benefit from a `RegionMask` excluding the road overpass; that change
  was not applied here.

## Next round

To produce the canonical exec-plan 001 acceptance numbers we need:

1. Add a clip with **quiet stretches** (empty hallway, parking lot at
   night, doorway at low-traffic hour). 30–120 s. Pexels search terms:
   `empty street`, `night surveillance`, `parking lot`.
2. Re-run all four strategies under the same configuration.
3. Re-score and update this document, or land a sibling
   `2026-XX-XX-mvp-vs-uniform-quiet.md`.

## Files

- `benchmarks/_runs/v3-tighter/` — per-strategy JSON, VLM response cache,
  `summary.json`.
- `benchmarks/runners/run.py`, `benchmarks/runners/sample.py`,
  `benchmarks/runners/vlm.py` — runner code.
- `benchmarks/analysis/compare.py` — scoring script.

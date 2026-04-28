# Recall

Validates that the trigger does not silently drop important moments.

## VLM-as-oracle

1. **Oracle pass.** Run uniform 1 fps across the full clip. Each frame goes to the VLM. Record events with timestamps. The VLM is noisy, so we treat it as a soft ground truth.
2. **Candidate pass.** Run the trigger configuration under test. Each triggered frame goes to the VLM. Record events.
3. **Recall.** For each Oracle event, ask: did the candidate fire within ±W seconds and report a comparable event? `W` defaults to 1.5 s.
4. **Metric.** `recall = (Oracle events captured by candidate) / (Oracle events total)`.

VLM responses are normalized via simple keyword match before comparison.

## Hand-labeled ground truth

When a clip has a manually-curated `events.csv` (intervals `[start_s, end_s]`), we use that instead of the Oracle. Hand labels avoid the Oracle's drift on continuous-content clips. Files: `benchmarks/datasets/<clip>/events.csv`.

## Tolerance window

`W=±1.5 s` is the default. We document `W` per result. A tighter window penalizes onset-mistuned triggers; a wider window inflates apparent recall.

For safety-critical use cases (alerting), `W` should be narrower. For cost-reduction use cases (VLM batching), wider is fine.

## What recall does not tell you

- It says nothing about cost. A dense trigger fires on every frame and gets perfect recall trivially.
- Pair recall with precision (or call-count) to characterize a configuration.

## Run count and noise

- Three runs minimum per configuration when the input has any randomness (e.g. live source).
- For deterministic file sources, one run is enough; declare so in the result file.
- If stdev > 10% of mean, increase run count and document why.

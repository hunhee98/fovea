# Precision

Fraction of triggered VLM calls that landed inside a labeled event.

## Definition

```
precision = (triggered frames within ±W of any event) / (triggered frames total)
```

Same `W` as recall (default ±1.5 s).

For a VLM cost-reduction trigger, precision is the load-bearing metric. Every call outside a real event is a wasted dollar. Recall caps how many calls you can skip; precision determines how many of the calls you do issue are useful.

## Comparison baselines

Precision is meaningful only relative to baselines. Standard fovea bench reports four:

- `oracle_1fps` — sets the ground truth. Precision is the same as the Oracle's own positive rate (i.e. fraction of Oracle frames that landed inside labeled events).
- `uniform_1fps` — same cadence as Oracle. Verifies cache reuse and per-call cost.
- `uniform_0.2fps` — sparser uniform sampling. The "naive cheap" baseline.
- `fovea_mv` — the trigger under test.

A useful trigger improves precision over `uniform_<same call count>fps`.

## What precision does not tell you

- It says nothing about whether you covered the timeline. A trigger that fires once at the dead center of a single 5-minute event has 100% precision and ~0% recall.
- Pair with recall.

## Edge cases

- **No labeled events.** Falls back to the Oracle's own keyword flag (`people OR vehicles` etc.). This is lenient and tends to inflate precision on continuous-content clips.
- **All-event clip.** Precision approaches 100% trivially. Document and treat the result as a sanity check, not a comparison.

# Cost

VLM API call count and dollar cost during a candidate run.

## Call count

Direct integer count of VLM invocations during a candidate pass. No averaging — report raw numbers per configuration.

```
calls_relative = candidate_calls / oracle_calls
```

Headline number for cost-reduction claims is `calls_relative`. Lower is better when paired with non-trivial recall.

## Dollar cost

Sum of per-call token cost at list prices for whatever VLM was used. Each result file declares:

- VLM model and version (e.g. "Gemini Flash latest, 2026-04-28").
- Input token bound (image size, JPEG quality).
- Token-pricing reference at the time of measurement.

Cost numbers age. They are anchored to the date in the result filename.

## What "cost" does not include

- Bandwidth (RTSP ingest is upstream of fovea).
- Storage (we do not record).
- Self-hosted VLM compute (when the user runs the model themselves, dollar cost is replaced by GPU-second cost — declare which).

## Caching policy

`benchmarks/_runs/cache/` caches VLM responses keyed on (model, image hash, prompt). Re-runs are free. We commit the cache directory in `.gitignore` (large), but the cache key is deterministic so the dollar cost reported in the result file is one-time, not per re-run.

## Why cost lives separate from latency / recall

Cost moves with vendor pricing and model updates. Latency and recall are properties of the trigger; cost is a property of the trigger × VLM × pricing. Keeping them separate avoids regressions caused by external pricing changes being mistaken for trigger regressions.

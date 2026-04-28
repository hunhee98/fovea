# Methodology

How fovea measures things. Skim this before reading any benchmark result file.

## Latency

- **Tool:** `criterion` for Rust microbenchmarks, custom Python timer for end-to-end.
- **Reported:** p50, p95, p99. Mean alone is meaningless for tail-sensitive code.
- **Excluded:** VLM API roundtrip (network-bound, varies by location). Reported separately when relevant.
- **Warmup:** at least 100 iterations before measurement. Cold start excluded.

## Recall@event (VLM-as-oracle)

To validate that fovea-mv does not silently drop important moments.

1. **Oracle pass:** run uniform 1fps on the full clip. Each frame → VLM. Record events with timestamps.
2. **Candidate pass:** run fovea-mv. Each triggered frame → VLM. Record events.
3. **Recall:** for each Oracle event, ask "did the candidate fire within ±2s and report a comparable event?"
4. **Metric:** recall = (Oracle events captured by candidate) / (Oracle events total).

VLM responses are normalized via simple keyword match before comparison (the oracle is noisy, so we tolerate phrasing drift).

## VLM call count

Direct count of API calls during a candidate run. No averaging — report the raw integer.

## Decode CPU%

- **Tool:** `psutil` sampling at 100ms intervals on the process.
- **Reported:** mean during idle windows (no Oracle events) and during active windows (Oracle event present).
- **Why split:** the value of fovea-mv is mostly in the idle window. A combined number obscures it.

## Hardware declaration

Every result file includes:

```
Hardware: <CPU model>, <RAM GB>, <GPU model or "none">, <OS version>
```

Without this, a number is unreproducible.

## Confidence and noise floor

- Three runs minimum per configuration.
- Report mean ± stdev.
- If stdev > 10% of mean, increase run count and document.

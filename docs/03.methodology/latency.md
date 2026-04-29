# Latency

## Per-packet latency

Time spent inside `fovea-trigger-core` parsing one H.264 / HEVC packet and updating trigger state.

- **Tool:** `criterion` microbench in `crates/fovea-trigger-core/benches/`.
- **Reported:** p50, p95, p99 in microseconds. Mean alone is meaningless for tail-sensitive code.
- **Warmup:** ≥ 100 iterations before measurement; cold start excluded.
- **Excluded:** decode, MV→RGB conversion, network read.

Per-packet latency is the metric that backs the "no decode in the idle path" claim. It must stay sub-millisecond on commodity CPU.

## Trigger fire latency

Time from the packet that *should* have fired the trigger to the moment `Stream` yields the corresponding event.

- **Tool:** Python harness in `benchmarks/runners/`. Wall-clock around the iterator.
- **Reported:** p50, p95, p99 in milliseconds.
- **Bound:** depends on GOP structure of the input — worst case is one full GOP. We document the GOP length used for each measurement.

This is what users feel. A 30-fps 60-frame GOP source means worst-case ~2 s; typical ~100 ms. Both must be reported separately.

## End-to-end latency (RTSP → emit)

For RTSP sources, includes network read, packet parse, trigger evaluation, and (if event) decode.

- **Tool:** end-to-end Python harness against a local MediaMTX loop or a real camera.
- **Excluded:** VLM inference. The point of this metric is the fovea overhead.
- **Reported:** p50, p95, p99 in milliseconds, separately for "trigger fire" (idle-window emit) and "first decoded frame" (event-window emit).

## What we do not measure

- VLM API roundtrip — too variable. Reported separately when relevant in the cost page.
- Disk I/O on file sources — not the load-bearing path.

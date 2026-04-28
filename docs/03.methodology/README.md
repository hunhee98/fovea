# Methodology

How fovea measures things. Skim before reading any `benchmarks/results/` file.

Each metric has its own page. Hardware declaration is mandatory across all of them — see `hardware.md`.

- [latency.md](latency.md) — per-packet, trigger fire, end-to-end.
- [recall.md](recall.md) — VLM-as-oracle, fraction of events captured.
- [precision.md](precision.md) — fraction of triggered calls landing inside labeled events.
- [density.md](density.md) — streams per box, idle CPU%, memory.
- [cost.md](cost.md) — VLM call count and dollar cost.
- [hardware.md](hardware.md) — required declaration, repro requirements.

---
name: bench
description: Run benchmark + compare to baseline + log results
---

For performance work.

1. Identify the canonical benchmark for the change (usually `cargo bench -p <crate>` or a Python harness in `benchmarks/`).
2. Run the benchmark on the **baseline** (origin/main or last release tag) — record numbers.
3. Run the benchmark on the **current branch** — record numbers.
4. Compute delta. Highlight regressions ≥ 5% or improvements ≥ 5%.
5. Persist the result as `benchmarks/results/<YYYY-MM-DD>-<slug>.md`:
   - Hardware line (CPU, RAM, GPU, OS) — required.
   - Commit hash being benchmarked.
   - Command line(s).
   - Numbers in a table.
   - Plot if the comparison has multiple data points.
6. Surface the result to the user. If the perf-critical files (`crates/fovea-trigger-core/`, `crates/fovea-trigger-stream/`) regressed > 5%, ask before proceeding to `/done`.

Every performance claim in code, docs, or commit messages must point back to a `benchmarks/results/` file.

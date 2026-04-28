---
name: review
description: Review current branch changes for correctness, perf, and citation accuracy
---

Read-only review of the current branch's diff vs `origin/main` (or the merge base).

Checks:

1. **Correctness** — diff makes sense; no obviously broken behavior; tests cover the change.
2. **Style** — no `Co-Authored-By` in commit messages; no "Generated with Claude Code"; no AI-style filler comments; commit messages explain *why*.
3. **Perf claims** — every numeric perf claim has a `benchmarks/results/` file referenced.
4. **Citations** — every cited paper / repo resolves and supports the cited number.
5. **License** — new dependencies are Apache-2.0 / MIT / BSD-compatible. LGPL only via dynamic linking with appropriate notice.
6. **Drift** — README, exec-plans, CHANGELOG match what shipped.
7. **API surface** — public types added are documented; clippy `missing-docs` would warn on undocumented public items.

Report findings as bullet list, ordered by severity. Don't auto-fix — surface, then let the user decide.

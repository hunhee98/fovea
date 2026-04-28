---
name: architecture
description: Architectural deepening — design docs and trade-off analysis
---

For "how should we design X" questions where there's no single obvious answer.

1. Read the current code so the analysis is grounded, not invented.
2. Lay out 2-4 viable approaches. For each:
   - Mechanism in plain words.
   - Cost (LOC, dependency, build time, runtime).
   - Risk (license, lock-in, complexity).
   - Cite prior art when applicable.
3. Recommend one — be opinionated, but explain trade-offs.
4. If the user accepts: persist the design as `docs/01.architecture/<slug>.md` (not an exec-plan; architecture lives separately).

Don't change source code in this skill. Architecture pass is read-only.

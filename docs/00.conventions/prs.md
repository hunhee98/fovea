# Pull Requests

Authoritative rules: `CLAUDE.md` → "Git → PR body".

Every PR body must answer three questions:

## 1. Claim

One line. What changes, in user-visible terms. No marketing, no superlatives.

> Bad: "Massively improves decode performance."
> Good: "Skip loop filter and IDCT during MV-only passes."

## 2. Evidence

Link to the test or benchmark that validates the claim.

- For `feat` / `fix`: link to the new unit / integration test.
- For `perf` / `bench`: **mandatory** link to `benchmarks/results/<date>-<slug>.md`.
- For `docs` / `chore`: write `n/a — no behavior change` and move on.

If the claim has a number, the number must come from a committed file in the same repo. No untracked benchmarks, no "I ran this on my laptop and got X".

## 3. Citation

If the change rests on a non-obvious technical decision drawn from prior work:

- Inline form: `[Author Year — arxiv:NNNN.NNNNN]` or `[Author Year — github.com/org/repo]`.
- If the work warrants its own note, link to `docs/02.papers/<arxiv-id>-<slug>.md`.
- If you can't verify the cited claim from the source, drop the number and rephrase qualitatively.

`/cite` skill + `cite-checker` agent verify this on the current branch.

## Reviewer checklist

The `review` skill runs through this. Worth knowing:

- Diff makes sense; tests cover the change.
- Commit messages follow `commits.md` (no banned trailers).
- Numeric perf claims point to `benchmarks/results/`.
- New deps are Apache-2.0 / MIT / BSD; LGPL only via dynamic linking.
- README / CHANGELOG / exec-plans match what shipped.
- Public API additions documented (clippy `missing-docs` would warn).

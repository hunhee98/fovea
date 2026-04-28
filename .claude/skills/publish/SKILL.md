---
name: publish
description: Publish a release to crates.io / PyPI
---

Only when the user explicitly asks. **Default posture is "side project, no release"** — do not auto-suggest publishing.

Preconditions before publishing:

- All tests + clippy green on the current branch.
- A `benchmarks/results/<date>-<slug>.md` file exists for the headline number being claimed.
- `CHANGELOG.md` has an entry for the new version with: features, fixes, perf deltas, breaking changes.
- Version tag follows `<subproject>-v<version>` (e.g. `fovea-mv-v0.1.0`).
- README / pyproject / Cargo.toml versions agree.

Steps:

1. Tag: `git tag <subproject>-v<version> -m "<short>"`.
2. Push tag: `git push origin <tag>`.
3. Crates: `cargo publish -p <crate>` (in dependency order).
4. PyPI: `maturin publish` from the package dir.
5. GitHub release: `gh release create <tag> --notes "<changelog excerpt>"`.

Stop and ask the user before any irreversible step (publish, push tag, force push). Never bypass with `--force` or `--no-verify`.

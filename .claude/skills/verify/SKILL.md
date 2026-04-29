---
name: verify
description: Run static checks and tests on the current branch
---

Run, in order, and stop on first failure:

1. `cargo check --workspace`
2. `cargo clippy --all-targets -- -D warnings`
3. `cargo test`
4. If `packages/fovea-trigger/` has changes: `cd packages/fovea-trigger && /Users/hunheelee/fovea/.venv/bin/maturin develop` then `/Users/hunheelee/fovea/.venv/bin/python -m pytest tests/`
5. `mypy --strict packages/` if user opted in (skip if mypy not installed)
6. `ruff check packages/` if user opted in

Report at the end:
- Total Rust test suites passing
- Total pytest cases passing
- Any warnings worth surfacing

If a step fails, stop and explain what failed and the fix path. Don't proceed to commit/push.

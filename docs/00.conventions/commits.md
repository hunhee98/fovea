# Commits

Authoritative rules: `CLAUDE.md` → "Git → Commits". Enforced by `scripts/git-hooks/commit-msg`.

## Subject

`<type>(<scope>): <subject>` — max 72 chars.

| type | when |
|------|------|
| `feat` | new functionality |
| `fix` | bug fix |
| `refactor` | structural change, no behavior delta |
| `docs` | docs only, zero `crates/` or `packages/` source change |
| `chore` | tooling, deps, config, CI |
| `bench` | new or updated benchmark |
| `perf` | performance improvement, must include before/after numbers in body |
| `test` | tests only |

`<scope>` is the subproject without prefix: `mv-core`, `mv-stream`, `mv-py`, `pick`, `stream`, `bench`, `docs`, `repo`, `examples`. Add new scopes when a new crate / area lands.

Korean or English subject acceptable, pick one per commit.

## Body

Explain *why*, not *what*. The diff already shows what.

For `perf` commits, body must include before/after numbers and a `benchmarks/results/<file>` link.

## Banned trailers — block by hook

- `Co-Authored-By: ...` — globally banned (per `~/.claude/CLAUDE.md`).
- `🤖 Generated with [Claude Code](...)` — banned.
- `Generated with Claude Code` — banned (any case).

`scripts/git-hooks/commit-msg` rejects these. `scripts/git-hooks/prepare-commit-msg` strips them defensively before validation, in case a tool inserts them.

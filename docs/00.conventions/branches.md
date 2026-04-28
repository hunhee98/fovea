# Branches

Authoritative rules: `CLAUDE.md` → "Git → Branches". Enforced by `scripts/git-hooks/pre-push`.

## Naming

`<type>/<kebab-slug>`. The `<type>` matches the Conventional Commits prefix 1:1.

- `feat/` `fix/` `refactor/` `docs/` `chore/` `bench/` `perf/`

Examples:

```
feat/mv-step2-h264-extraction
perf/mv-core-decode-skip-flags
bench/density-multi-stream
docs/conventions-extract
```

## main protection

- `main` is PR-only. **Never** push, force-push, or merge directly.
- Merging a PR into main requires explicit user approval. AI does not click merge.
- Three layers enforce this:
  1. `.claude/settings.json` deny rule blocks `git push origin main` at the harness layer.
  2. `.claude/hooks/pre-tool-bash-main-guard.sh` catches broader mutation patterns and surfaces a clearer error.
  3. `scripts/git-hooks/pre-push` rejects any push whose remote ref is `refs/heads/main`.

If you need to fix something on main, the path is: feature branch → PR → ask user to merge.

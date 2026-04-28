# Orchestration

How fovea uses Claude Code as its orchestrator. The user describes work in natural language; the harness routes that into skills, subagents, and evidence-bearing artifacts.

This document is the map. Individual skills (`.claude/skills/<name>/SKILL.md`) and subagents (`.claude/agents/<name>.md`) are the leaves.

## Layers

```
                  ┌────────────────────────────────────────────┐
   user (NL) ─▶   │  CLAUDE.md — rules, intent table           │
                  └────────────────────┬───────────────────────┘
                                       │ classify intent
                  ┌────────────────────▼───────────────────────┐
                  │  Skill (.claude/skills/<name>/SKILL.md)    │
                  │  feature / fix / verify / bench / done ... │
                  └────────────────────┬───────────────────────┘
                                       │ delegate when context-heavy
                  ┌────────────────────▼───────────────────────┐
                  │  Subagent (.claude/agents/<name>.md)       │
                  │  bench-runner / mv-researcher / cite       │
                  └────────────────────┬───────────────────────┘
                                       │ produce
                  ┌────────────────────▼───────────────────────┐
                  │  Evidence                                  │
                  │  benchmarks/results/ · tests · paper notes │
                  └────────────────────┬───────────────────────┘
                                       │ enforced by
                  ┌────────────────────▼───────────────────────┐
                  │  Hooks                                     │
                  │  .claude/hooks/ · scripts/git-hooks/       │
                  └────────────────────────────────────────────┘
```

## The loop

1. **Classify intent.** `CLAUDE.md → "Orchestration"` has the intent → skill table. The harness reads the user's NL and picks a skill.
   - HIGH confidence (unique match): silent invoke + 1-line announce.
   - MEDIUM (2–3 candidates, clear winner): announce choice, allow redirect, invoke.
   - LOW (off-table or ambiguous): one clarifying question, do not invoke yet.

2. **Run the skill.** Each skill's `SKILL.md` is self-contained. It owns its happy-path and its MUST rules. Skills compose: most end with `/verify` then `/done`.

3. **Delegate to subagents when needed.** Subagents exist to keep heavy output out of the parent context.
   - `bench-runner` — run criterion / Python harness, write `benchmarks/results/<file>`, return a 4-line summary.
   - `mv-researcher` — read codec / decoder code paths, return file:line references and a plain-English answer.
   - `cite-checker` — verify arxiv IDs and claims, flag malformed citations, never auto-fix.

4. **Produce evidence, not prose.** Performance claims live in `benchmarks/results/`. Behavioral claims live in tests. Paper-derived claims live in `docs/02.papers/`. The PR body links to these artifacts; it does not contain the numbers.

5. **Hooks enforce the rules.** Hooks turn rules into automation:
   - Claude hooks (`.claude/hooks/`) trigger on session start, tool use, and stop. They surface state and gentle warnings.
   - Git hooks (`scripts/git-hooks/`) trigger on commit / push. They block hard violations (banned trailers, wrong branch prefix, push to main).
   - Settings deny rules (`.claude/settings.json`) block dangerous bash patterns at the harness layer.

## Why three hook layers

| layer | runs on | enforces | example |
|-------|---------|----------|---------|
| settings.json deny | tool dispatch | absolute prohibitions | `git push origin main*` |
| `.claude/hooks/` | Claude lifecycle events | state surfacing, soft warnings | "uncommitted bench results" |
| `scripts/git-hooks/` | git commit / push | format / content rules with hard-block | Conventional Commits, banned trailers |

Each catches a distinct class of failure. None alone is sufficient — settings catches automation, Claude hooks catch in-session drift, git hooks catch human commits done outside the harness.

## What lives where (state durability)

| lifetime | medium |
|----------|--------|
| within a session | conversation context, todos |
| across sessions on the same line of work | PR body |
| multi-session, deferred, or reference | `docs/05.exec-plans/<NNN>-<slug>.md` |
| persistent judgment / preferences | `.claude/memory/` |
| measurable claims | `benchmarks/results/<date>-<slug>.md` |

`.claude/memory/` stores judgment criteria only — feedback, decisions, references. It does not store work state. Work state belongs in PRs or exec-plans.

## Failure modes the hooks catch

These are the patterns we have seen drift in across sessions:

- Banned trailers slipping into commit messages — `scripts/git-hooks/commit-msg` blocks; `scripts/git-hooks/prepare-commit-msg` strips defensively.
- Performance claims in commit bodies without a `benchmarks/results/` file — `.claude/hooks/post-tool-edit-hotpath.sh` reminds; `scripts/git-hooks/pre-push` warns.
- Exec-plans ending up status-less — `.claude/hooks/post-tool-edit-execplan.sh` warns immediately; `scripts/git-hooks/pre-commit` blocks if missing.
- Silent allocation in the hot path — `.claude/hooks/post-tool-edit-hotpath.sh` and `scripts/git-hooks/pre-commit` grep for known patterns; whitelist with `// alloc-ok: <reason>`.
- Pushes to `main` — three layers (settings deny, Claude hook, git pre-push).
- Stale exec-plans (`Status: in progress` for > 14 days) — `.claude/hooks/stop-summary.sh` surfaces at session end.

## Reading order for new contributors

1. `CLAUDE.md` (root) — rules.
2. This file — how rules become automation.
3. `docs/00.conventions/` — concrete forms (commits, branches, PRs, citations).
4. `docs/03.methodology/` — how we measure.
5. `.claude/skills/<name>/SKILL.md` — happy paths.

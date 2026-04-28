---
name: refactor-plan
description: Plan a refactor as a sequence of tiny, individually-buildable commits
---

For "rename / extract / reorganize / restructure" work.

Output: a bullet list of commits, each:
- ≤ ~150 LOC diff.
- Independently buildable + clippy-clean.
- Contains its own test if behavior changes.
- Has a Conventional Commits subject.

Order them so reviewers (or `git bisect`) can land them one at a time without intermediate breakage. Don't implement in this skill — produce the plan, then let the user invoke `/feature` (or just continue) to execute.

If the user accepts the plan, persist it to `docs/05.exec-plans/<NNN>-<slug>.md` so we can track progress across sessions.

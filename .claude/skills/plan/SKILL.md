---
name: plan
description: Produce an implementation plan — chat output or persisted to docs/05.exec-plans/
---

Plan-only. Do not write or change source code in this skill.

When to persist to `docs/05.exec-plans/<NNN>-<slug>.md`:
- Multi-session work likely.
- Reference-worthy beyond the current PR body.

When to keep it in chat:
- Single-session work, fits in PR body.

Format for a persisted exec-plan:

```
# NNN — <title>

Status: in progress / parked / completed YYYY-MM-DD — <PR ref> / speculative
Owner: @<handle>
Subproject: `<name>`

## Context
why this work, what triggered it.

## Goals
bullet list.

## Non-goals
what we are explicitly not doing.

## Approach
ordered steps. Each step ends with a verifiable result (test, bench, observation).

## Acceptance
checkboxes that gate "done".

## Risks
known unknowns + mitigations.

## History
| Date | Event | Reference |
```

Keep it concise — exec-plans are working docs, not whitepapers. Prefer numbered concrete steps over prose.

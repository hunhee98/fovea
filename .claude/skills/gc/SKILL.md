---
name: gc
description: Detect and clean up drift introduced by agent-generated code
---

Periodic cleanup pass. Look for:

- Stale `// TODO(claude)` / `// FIXME` markers without an owning issue.
- Dead code: functions / types unused outside tests.
- Stale exec-plans in `docs/05.exec-plans/` whose Status is still "in progress" but the work shipped weeks ago.
- README / CHANGELOG drift vs what's actually implemented.
- Cargo features no longer wired up to anything.
- gitignored files that have been unintentionally re-added.

Don't delete user content (memory files, exec-plans the user is actively editing). When in doubt, surface findings to the user before removing.

End with `/verify` to confirm the cleanup didn't break anything, then `/done` if changes are worth committing.

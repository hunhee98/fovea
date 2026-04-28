# Conventions

Single source of truth is `CLAUDE.md` at repo root. The files here are short pointers grouped by topic, so a reviewer can grep without reading the full CLAUDE.md.

Most of these rules are also enforced by hooks under `scripts/git-hooks/` and `.claude/hooks/`. If a hook blocks you, the rule is one of these files.

- [commits.md](commits.md) — Conventional Commits, banned trailers.
- [branches.md](branches.md) — branch prefixes, main protection.
- [prs.md](prs.md) — PR body shape (Claim / Evidence / Citation).
- [citations.md](citations.md) — citation format and verification.

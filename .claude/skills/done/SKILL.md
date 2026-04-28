---
name: done
description: Commit and push the current branch (after /verify passed)
---

Preconditions:
- `/verify` has been run and is green.
- Working tree has changes to commit.
- Branch is **not** `main` — never commit/push directly to main.

Steps:

1. `git status` — confirm working tree state.
2. `git diff` — sanity check the diff is what we intended.
3. `git log --oneline -5` — check style of recent commit messages.
4. Stage explicitly by name (no `git add -A`).
5. `git commit -m "<subject>" -m "<body>"`:
   - Conventional Commits prefix: `feat` / `fix` / `refactor` / `docs` / `chore` / `bench` / `perf` / `test`.
   - Scope: subproject without prefix — `(mv-core)` `(mv-py)` `(stream)` `(pick)` `(bench)` `(docs)`.
   - **No** `Co-Authored-By` lines, **no** "Generated with Claude Code" footer.
   - Body explains the *why*, not the *what*.
6. `git push origin <branch>` — only ever the current feature branch, never main.
7. Report PR URL if GitHub printed one.

If the user asks to merge to main: do not do it directly. Open a PR via `gh pr create` and let the user merge it themselves.

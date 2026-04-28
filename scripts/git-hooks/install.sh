#!/usr/bin/env bash
# Install fovea git hooks. Idempotent — re-run anytime to refresh.
set -euo pipefail

repo_root=$(git rev-parse --show-toplevel)
src="$repo_root/scripts/git-hooks"
dst="$repo_root/.git/hooks"

mkdir -p "$dst"

for hook in commit-msg pre-commit pre-push prepare-commit-msg; do
  ln -sfn "../../scripts/git-hooks/$hook" "$dst/$hook"
  chmod +x "$src/$hook"
  printf 'installed: %s\n' "$dst/$hook"
done

printf '\nfovea git hooks installed. Verify with: git config core.hooksPath (should be unset; we use symlinks in .git/hooks).\n'

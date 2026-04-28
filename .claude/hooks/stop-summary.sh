#!/usr/bin/env bash
# Stop — end-of-session reminders. Stale plans, uncommitted bench results.
set -u
root=$(git rev-parse --show-toplevel 2>/dev/null) || exit 0
cd "$root" || exit 0

# Uncommitted benchmarks/results — perf claims must commit alongside code (CLAUDE.md).
if [ -n "$(git status --porcelain benchmarks/results/ 2>/dev/null)" ]; then
  printf '%s\n' "[fovea hook] uncommitted benchmarks/results/ — commit before /done."
fi

# Stale in-progress exec-plans (> 14 days untouched).
now=$(date +%s)
for f in docs/05.exec-plans/*.md; do
  [ -f "$f" ] || continue
  grep -qE '^Status:[[:space:]]*in progress' "$f" 2>/dev/null || continue
  mtime=$(stat -f %m "$f" 2>/dev/null || stat -c %Y "$f" 2>/dev/null || echo "$now")
  age_days=$(( (now - mtime) / 86400 ))
  if [ "$age_days" -gt 14 ]; then
    printf '%s\n' "[fovea hook] stale: $f (in progress, $age_days days old) — update Status or move to archive."
  fi
done

# Hot-path edits without bench result update.
hotpath_changed=$(git diff --cached --name-only 2>/dev/null; git diff --name-only 2>/dev/null)
if printf '%s' "$hotpath_changed" | grep -q 'crates/fovea-mv-core/'; then
  bench_changed=$(printf '%s' "$hotpath_changed" | grep 'benchmarks/results/' || true)
  if [ -z "$bench_changed" ]; then
    printf '%s\n' "[fovea hook] mv-core changed but no benchmarks/results/ update — run 'make density-bench'."
  fi
fi

exit 0

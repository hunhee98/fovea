#!/usr/bin/env bash
# PostToolUse Edit/Write — exec-plan must carry a Status: line (CLAUDE.md rule).
set -u
input=$(cat)
path=$(printf '%s' "$input" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('tool_input',{}).get('file_path',''))" 2>/dev/null || echo "")

case "$path" in
  */docs/05.exec-plans/*.md) ;;
  *) exit 0 ;;
esac

[ -f "$path" ] || exit 0

if ! head -10 "$path" | grep -qE '^Status:[[:space:]]+'; then
  printf '%s\n' "[fovea hook] WARN: ${path#${PWD}/} missing 'Status:' line in first 10 lines."
  printf '%s\n' "  → add one of: 'Status: in progress' / 'Status: completed YYYY-MM-DD — <PR>' / 'Status: deferred — <reason>'"
fi
exit 0

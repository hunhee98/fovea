#!/usr/bin/env bash
# PreToolUse Bash — block direct main mutation.
# Settings deny covers some patterns; this catches broader forms with clearer message.
set -u
input=$(cat)
cmd=$(printf '%s' "$input" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('tool_input',{}).get('command',''))" 2>/dev/null || echo "")

if printf '%s' "$cmd" | grep -qE 'git[[:space:]]+push[[:space:]]+([^|;&]+[[:space:]]+)?main\b|git[[:space:]]+push[[:space:]]+--force[[:space:]]+.*main\b|git[[:space:]]+push[[:space:]]+origin[[:space:]]+main\b'; then
  printf '%s\n' "[fovea hook] blocked: direct push to main. open PR via gh pr create." >&2
  exit 2
fi

if printf '%s' "$cmd" | grep -qE 'git[[:space:]]+merge[[:space:]]+.*[[:space:]]main\b|git[[:space:]]+checkout[[:space:]]+main[[:space:]]*[;&]+.*git[[:space:]]+(commit|merge|push)'; then
  printf '%s\n' "[fovea hook] blocked: merge/commit on main. PR-only flow." >&2
  exit 2
fi

exit 0

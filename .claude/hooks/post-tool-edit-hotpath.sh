#!/usr/bin/env bash
# PostToolUse Edit/Write — hot-path alloc patterns + density-bench reminder.
# Trigger only on crates/fovea-trigger-core/ .rs files.
set -u
input=$(cat)
path=$(printf '%s' "$input" | python3 -c "import sys,json; d=json.load(sys.stdin); print(d.get('tool_input',{}).get('file_path',''))" 2>/dev/null || echo "")

case "$path" in
  */crates/fovea-trigger-core/*.rs) ;;
  *) exit 0 ;;
esac

[ -f "$path" ] || exit 0

# Allocation patterns. Whitelist with `// alloc-ok: <reason>` on same line.
allocs=$(grep -nE 'Box::new\(|vec!\[|String::from\(|format!\(|\.to_owned\(\)|\.to_string\(\)' "$path" \
  | grep -v 'alloc-ok:' \
  | head -5)

if [ -n "$allocs" ]; then
  printf '%s\n' "[fovea hook] hot-path alloc patterns in ${path#${PWD}/}:"
  printf '%s\n' "$allocs"
  printf '%s\n' "  → if intentional, append '// alloc-ok: <reason>' on the line."
fi

printf '%s\n' "[fovea hook] mv-core edited — run 'make density-bench' before /done. Regression > 5% blocks pre-push."
exit 0

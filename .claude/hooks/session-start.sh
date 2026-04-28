#!/usr/bin/env bash
# SessionStart — fovea repo state banner. Cheap, runs every session.
set -u
root=$(git rev-parse --show-toplevel 2>/dev/null) || exit 0
cd "$root" || exit 0

branch=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo "?")
dirty=$(git status --porcelain 2>/dev/null | wc -l | tr -d ' ')
last_bench=$(ls -t benchmarks/results/*.md 2>/dev/null | head -1 | xargs -I{} basename {} .md 2>/dev/null || echo "none")
in_progress_plans=$(grep -lE '^Status:\s*in progress' docs/05.exec-plans/*.md 2>/dev/null | wc -l | tr -d ' ')

cat <<EOF
[fovea] branch=$branch dirty=$dirty last_bench=$last_bench in_progress_plans=$in_progress_plans
[fovea] rules: no Co-Authored-By, no main push, perf claims must link benchmarks/results/<date>-<slug>.md
[fovea] supported codecs: H.264 + HEVC only. MJPEG/AV1/VP9 must error explicitly.
EOF

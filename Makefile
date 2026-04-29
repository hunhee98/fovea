# fovea Makefile — single entry point for verify, bench, hooks, gc.
# Skills (.claude/skills/) shell out here so commands stay consistent.

PY := .venv/bin/python
MATURIN := .venv/bin/maturin

.PHONY: help
help:
	@grep -E '^[a-zA-Z_-]+:.*?## ' $(MAKEFILE_LIST) | awk -F':.*## ' '{printf "  %-22s %s\n", $$1, $$2}'

# ---------------------------------------------------------------------------
# Hooks
# ---------------------------------------------------------------------------

.PHONY: install-hooks
install-hooks: ## Install fovea git hooks into .git/hooks (idempotent)
	@bash scripts/git-hooks/install.sh

.PHONY: install-bench-deps
install-bench-deps: ## Install Python deps for benchmarks/ (psutil, dotenv, ...)
	$(PY) -m pip install -r benchmarks/requirements.txt

# ---------------------------------------------------------------------------
# Verify (matches /verify skill)
# ---------------------------------------------------------------------------

.PHONY: verify
verify: verify-rust verify-py ## Run all static checks + tests on current branch

.PHONY: verify-rust
verify-rust: ## cargo check + clippy + test
	cargo check --workspace
	cargo clippy --all-targets -- -D warnings
	cargo test

.PHONY: verify-py
verify-py: ## Build PyO3 + run pytest on packages/fovea-trigger
	cd packages/fovea-trigger && $(abspath $(MATURIN)) develop
	$(PY) -m pytest packages/fovea-trigger/tests/

# ---------------------------------------------------------------------------
# Benchmarks
# ---------------------------------------------------------------------------

.PHONY: bench
bench: ## Run the canonical accuracy bench (precision / recall / cost on cctv-sample)
	$(PY) -m benchmarks.runners.run \
	    benchmarks/datasets/cctv-sample/sample.mp4 \
	    --run-id verify-bench \
	    --motion-threshold 200000 --interval-ms 10000 --scene-threshold 0.6
	$(PY) -m benchmarks.analysis.compare benchmarks/_runs/verify-bench

.PHONY: density-bench
density-bench: ## Multi-stream density bench (idle CPU%, RSS, sustainable streams)
	$(PY) -m benchmarks.runners.density \
	    --source benchmarks/datasets/cctv-sample/sample.mp4 \
	    --streams 1,4,16 \
	    --duration-s 30

.PHONY: density-bench-large
density-bench-large: ## Density bench up to N=64 — slow, run when targeting 0.2 acceptance
	$(PY) -m benchmarks.runners.density \
	    --source benchmarks/datasets/cctv-sample/sample.mp4 \
	    --streams 1,4,16,32,64 \
	    --duration-s 60

.PHONY: bench-baseline
bench-baseline: ## Run bench against origin/main and current branch, print delta
	@bash -c 'set -euo pipefail; \
	  current=$$(git rev-parse --abbrev-ref HEAD); \
	  test "$$current" != "main" || (echo "already on main"; exit 1); \
	  git stash --include-untracked --quiet || true; \
	  git checkout origin/main --quiet; \
	  $(MAKE) -s bench || true; \
	  cp -r benchmarks/_runs/verify-bench benchmarks/_runs/_baseline; \
	  git checkout "$$current" --quiet; \
	  git stash pop --quiet || true; \
	  $(MAKE) -s bench; \
	  echo "baseline: benchmarks/_runs/_baseline   branch: benchmarks/_runs/verify-bench"; \
	  echo "compare manually until the diff harness lands."'

# ---------------------------------------------------------------------------
# Citation check
# ---------------------------------------------------------------------------

.PHONY: cite-check
cite-check: ## Grep for malformed inline citations across the repo
	@bash -c 'set -e; \
	  bad=$$(grep -rnE "\[arxiv[: ][0-9]+\.[0-9]+\]" --include="*.md" --include="*.rs" --include="*.py" . 2>/dev/null | grep -v "^./target/" | grep -v "^./.venv/" || true); \
	  if [ -n "$$bad" ]; then \
	    echo "malformed citations:"; echo "$$bad"; exit 1; \
	  else \
	    echo "no malformed citations found."; \
	  fi'

# ---------------------------------------------------------------------------
# GC (matches /gc skill)
# ---------------------------------------------------------------------------

.PHONY: gc
gc: ## Surface drift: stale exec-plans, dead TODOs, missing Status lines
	@echo "== exec-plans without Status: =="
	@for f in docs/05.exec-plans/*.md; do \
	  test -f "$$f" || continue; \
	  case "$$f" in *archive*) continue ;; esac; \
	  head -10 "$$f" | grep -qE '^Status:' || echo "  $$f"; \
	done
	@echo ""
	@echo "== stale 'in progress' plans (mtime > 14 days) =="
	@now=$$(date +%s); \
	for f in docs/05.exec-plans/*.md; do \
	  test -f "$$f" || continue; \
	  grep -qE '^Status:[[:space:]]*in progress' "$$f" 2>/dev/null || continue; \
	  mtime=$$(stat -f %m "$$f" 2>/dev/null || stat -c %Y "$$f" 2>/dev/null || echo "$$now"); \
	  age=$$(( (now - mtime) / 86400 )); \
	  test "$$age" -gt 14 && echo "  $$f ($$age days)"; \
	done
	@echo ""
	@echo "== TODO(claude) markers =="
	@grep -rn 'TODO(claude)' --include="*.rs" --include="*.py" --include="*.md" . 2>/dev/null | grep -v target/ | grep -v .venv/ || echo "  none"

# ---------------------------------------------------------------------------
# Convenience
# ---------------------------------------------------------------------------

.PHONY: clean
clean: ## Remove cargo target/ and Python __pycache__
	cargo clean
	find . -name '__pycache__' -type d -prune -exec rm -rf {} +

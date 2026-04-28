# Fovea Dev Guide

**Conversation language: Korean.** Respond in Korean always. Code, docs, and CLAUDE.md remain English-only per the rule below.
**All docs/ + CLAUDE.md — English only, written to be unambiguous and easy for Claude to parse: concrete, specific, no filler.** User asking to "respond in Korean" applies to conversation only — never to internal documents. Code comments may be Korean.

## What this is

**Fovea is an evidence-driven infrastructure OSS** for video → VLM pipelines. Single-developer project. Goal: production-grade Rust core with measurable, reproducible claims.

**Identity:** engineering OSS (like vLLM, candle, Frigate), not academic. We cite papers when relevant for credibility, not because we publish papers.

## Project (umbrella → subprojects)

Fovea is a monorepo. Subprojects ship as independent crates/packages but share infra.

| Subproject | Status | Role |
|------------|--------|------|
| `fovea-mv` (Phase 1) | active | H.264 motion-vector trigger engine. Decides which frames deserve downstream processing. Rust core + PyO3. |
| `fovea-pick` (Phase 2) | planned | Hybrid frame sampler. MV trigger + CLIP/DINOv2 diff + token-budget selection. |
| `fovea-stream` (Phase 3) | planned | Streaming VLM wrapper. Two-process split (Flash-VStream pattern) + KV cache reuse (StreamingVLM pattern) over HF VLMs. |

→ Architecture overview: `docs/01.architecture/OVERVIEW.md`
→ Cited papers: `docs/02.papers/`
→ Benchmark methodology: `docs/03.methodology/`
→ Active execution plans: `docs/05.exec-plans/`

## Critical Rules (Global)

### Evidence rules

- **Every performance claim must point to a benchmark file in `benchmarks/results/`.** No "fast", "efficient", "low-latency" without a number tied to a reproducible run.
- **Every non-obvious technical decision must cite a paper or established prior art.** Inline citation: `[CoViAR — arxiv 1712.00636]`. Paper PDFs/notes go in `docs/02.papers/`.
- **Benchmarks before features.** When adding a feature whose value is performance, the benchmark proving the claim must land in the same PR.
- **No regression-blind merges.** Performance-critical files (`crates/fovea-mv-core/`, `crates/fovea-mv-stream/`) require a perf check on PR. If numbers regress >5% on the canonical benchmark, the PR must justify or fix.

### Reproducibility rules

- **Benchmarks must be self-contained.** Any user with a clean clone + `make bench` runs the same numbers. No "you need this proprietary video".
- **Datasets are scripted.** `benchmarks/datasets/<name>/download.sh` downloads from public sources (Pexels, public CCTV samples, open-licensed clips). No commit of large media files.
- **Results are committed.** `benchmarks/results/<YYYY-MM-DD>-<slug>.md` — versioned, dated, includes commit hash, hardware, command, numbers, plot.
- **Hardware is declared.** Every result file has a `Hardware:` line (CPU, RAM, GPU, OS). Without it the number is meaningless.

### Code rules

- **Rust: no unsafe without justification comment.** `// unsafe: <why this is sound>` required.
- **Rust: `#![warn(missing_docs)]` on all public crates.** Public API has rustdoc.
- **Python: type-checked.** `mypy --strict` clean on `packages/`.
- **No silent allocations in hot path.** Per-packet code path (MV aggregation) must not allocate. Bench-enforced.
- **No third-party AI runtime in core crates.** `fovea-mv-core` does not depend on torch/onnx/candle. Its job is parsing and arithmetic, period.

### Citation rules

- Inline citation format: `[Author Year — arxiv:NNNN.NNNNN]` or `[Author Year — github.com/org/repo]`.
- Paper notes live at `docs/02.papers/<arxiv-id>-<short-title>.md` with: tldr, method, numbers, why it matters here.
- When user asks "where did this number come from" the answer must be a path or URL, not "I read it somewhere".

## Repo

```
fovea/
├── README.md                  # Public-facing intro + headline numbers
├── CITATION.cff               # Academic citation metadata
├── CHANGELOG.md               # Version history + perf deltas
├── LICENSE                    # Apache-2.0
├── CLAUDE.md                  # This file
├── Cargo.toml                 # Rust workspace root
├── pyproject.toml             # Python workspace
│
├── crates/                    # Rust core
│   ├── fovea-mv-core/         # H.264 NAL + MV parsing, triggers
│   ├── fovea-mv-stream/       # Source adapters (file, RTSP, ...)
│   └── fovea-mv-py/           # PyO3 bindings
│
├── packages/                  # Python distributions
│   └── fovea-mv/              # Python wrapper around fovea-mv-py
│
├── benchmarks/                # First-class — see Critical Rules
│   ├── README.md              # How to reproduce
│   ├── datasets/              # download.sh scripts (no media committed)
│   ├── baselines/             # uniform-sampling, mv-extractor, etc.
│   ├── runners/               # fovea-mv runs
│   ├── analysis/              # comparison + plotting
│   └── results/               # dated, committed
│
├── docs/
│   ├── 00.conventions/        # Long-form rules (CLAUDE.md is short)
│   ├── 01.architecture/       # Design docs
│   ├── 02.papers/             # Cited paper notes
│   ├── 03.methodology/        # Benchmark methodology
│   └── 05.exec-plans/         # Active work plans (status-tracked)
│
├── examples/                  # User-facing demos
│   ├── youtube_demo.py
│   └── rtsp_demo.py
│
└── .claude/                   # AI agent config
    ├── settings.json
    ├── skills/
    ├── hooks/
    ├── agents/
    └── memory/
```

## Versioning & Release

- **Semantic versioning per subproject.** `fovea-mv-0.1.0`, `fovea-stream-0.2.1`, etc. No monorepo-wide version.
- **0.x means API may change.** 1.0 means we promise stability.
- **Release artifacts:** Rust crates → crates.io, Python packages → PyPI. Both via tagged release.
- **Tag format:** `<subproject>-v<version>` (e.g. `fovea-mv-v0.1.0`).
- **Every release must update `CHANGELOG.md`** with: features, fixes, perf deltas (with bench result links), breaking changes.

## Git

**Branches:** `feature/` `fix/` `refactor/` `docs/` `chore/` `bench/` `perf/` (kebab-case after slash). Branch prefix matches commit prefix 1:1 (Conventional Commits).
- `bench/` — adding or updating benchmarks (separate from `feat/` because benchmarks gate other work).
- `perf/` — performance optimizations, must include before/after bench numbers in the PR.
- `docs/` — only when zero `crates/` / `packages/` source changes.
- `chore/` — config, tooling, deps, CI; no source.

**Strategy:**
- `main` — PR only, no direct push. **Merge requires explicit user approval.**
- All work via feature branch → PR → main. No `develop` branch (single dev, no SaaS topology).

**Commits:**
- Korean subject + body acceptable. English subject + body acceptable. Pick one per commit.
- Commit prefixes: `feat` `fix` `refactor` `docs` `chore` `bench` `perf` `test`.
- Scope: subproject name without prefix — `(mv-core)` `(mv-py)` `(stream)` `(pick)` `(bench)` `(docs)`.
- No `Co-Authored-By` or "Generated with Claude Code" footers.
- `.claude/` contents (skills/, hooks/, agents/, memory/) always staged when changed.

**PR body:** Every PR must answer:
1. **Claim** — what changes, in one line.
2. **Evidence** — link to bench result or test that validates the claim. For `perf/` PRs this is mandatory.
3. **Citation** — papers/repos referenced, if any.

## Testing Policy

Three layers:

**Rust unit tests (`cargo test`):**
- Pure-function tests for parsers, aggregators, trigger logic.
- Must run in <30s on the full workspace.
- Property tests (proptest) for parser fuzzing — corrupt H.264 input should never panic.

**Python integration tests (`pytest`):**
- End-to-end through PyO3 bindings on small sample MP4s.
- Sample MP4s ≤ 5MB total, committed.

**Benchmarks (`make bench`):**
- Live in `benchmarks/`, NOT next to code.
- Three required dimensions per claim: latency, throughput, accuracy (recall@event when applicable).
- Results committed to `benchmarks/results/<date>-<slug>.md`.

**Out of scope:**
- E2E browser tests — irrelevant (library, no UI).
- Visual regression — irrelevant.
- Component/UI tests — irrelevant.

## Verification

- **AI verifies:** `cargo check`, `cargo clippy -D warnings`, `cargo test`, `pytest`, `mypy --strict`, `ruff`, build success, benchmark regression check.
- **User verifies:** subjective design choices (API ergonomics), final benchmark interpretation, release timing.

## Planning

Planning state lives in one of three places by lifetime:

| Lifetime | Medium |
|----------|--------|
| Within a session | Conversation context + agent prompts |
| Across sessions on the same line of work | PR body |
| Multi-session, deferred, or reference-worthy | `docs/05.exec-plans/<NNN>-<slug>.md` |

**Exec-plan lifecycle — strictly enforced:**
Every exec plan MUST carry a `Status:` line at the top (`Status: in progress`, `Status: completed YYYY-MM-DD — <PR ref>`, `Status: speculative / not adopted`, `Status: deferred — <reason>`). When work referenced by a plan ships, the same PR (or an immediate `docs/` follow-up) updates the plan. When all roadmap items are done, move to `docs/05.exec-plans/archive/<slug>.md`. Skills that wrap up work (`/done`, `/release`) check whether merged PRs reference an exec plan slug and prompt the user to update.

**Exec-plan numbering:** Three-digit zero-padded prefix in chronological order (`001-mvtrigger-mvp.md`, `002-rtsp-source.md`, ...). New plans get the next number.

**Memory purpose — strictly enforced:**
Memory stores **learned judgment criteria only** — feedback, user preferences, decision rationale, external references. Never store work state, task lists, progress tracking, or session-specific context in memory. Work state belongs in PR bodies (cross-session) or `docs/05.exec-plans/` (persistent).

Memory lives at `.claude/memory/` (synced via repo). Set via `.claude/settings.json` (`autoMemoryDirectory: "./.claude/memory"`).

## Orchestration

When user describes a task in natural language, classify intent and apply the matching skill automatically — do not wait for slash commands.

### Intent → skill table (authoritative)

| Intent | Skill |
|--------|-------|
| New functionality / feature | `Skill("feature")` |
| Bug / broken behavior | `Skill("fix")` |
| Code cleanup / drift check | `Skill("gc")` |
| Refactor planning — tiny commits | `Skill("refactor-plan")` |
| Architectural deepening | `Skill("architecture")` |
| Static checks + tests on current branch | `Skill("verify")` |
| Run benchmark + compare to baseline | `Skill("bench")` |
| Add or verify citations | `Skill("cite")` |
| Design ablation study | `Skill("ablate")` |
| Commit and push (after `/verify` passes) | `Skill("done")` |
| Plan only — no implementation | `Skill("plan")` |
| Review only — current branch | `Skill("review")` |
| Publish release to crates.io / PyPI | `Skill("publish")` |

### Common rules across all skills

- **Ask ONE then explore.** When information is missing, ask at most one clarifying question, then read code/docs to fill the rest.
- **Default to action.** When intent is HIGH or MEDIUM confidence, invoke and proceed.
- **Self-contained SKILL.md.** Every skill's `SKILL.md` contains all MUST rules and the happy-path flow.

### Routing confidence — 3-tier

```
[user NL] → orchestrator
            ├─ HIGH (unique skill description match)   → silent invoke + 1-line announce
            ├─ MEDIUM (2-3 candidates, clear winner)   → "/X로 진행할게요. 다른 거 원하면 말씀하세요." + invoke
            └─ LOW (off-table or ambiguous)            → ask ONE clarifying question, do not invoke yet
```

## What Fovea is NOT

To prevent scope creep:

- **Not a model.** We do not train, fine-tune, or distribute weights. We are inference infrastructure.
- **Not a NVR.** We do not record, manage cameras, or replace Frigate. We are a library used by NVRs.
- **Not a UI.** We do not ship dashboards. Integrators build UIs on top.
- **Not a service.** No SaaS, no hosted endpoint. OSS library only.
- **Not a Python-first project.** Python is the bindings layer. The core is Rust. Performance claims live in Rust.

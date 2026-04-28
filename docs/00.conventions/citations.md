# Citations

Authoritative rules: `CLAUDE.md` → "Citation rules". Enforcement: `scripts/git-hooks/pre-commit` (format), `cite-checker` agent (resolution + claim support).

## Inline format

```
[Author Year — arxiv:NNNN.NNNNN]
[Author Year — github.com/org/repo]
```

`Author` is first-author surname. Year is publication year. `arxiv:NNNN.NNNNN` keeps the `arxiv:` prefix and uses the canonical 4–5 digit dot 5 digit form.

Bad forms (pre-commit warns):

```
[arxiv 1712.00636]                  # missing author/year
[He et al. — 1712.00636]            # missing arxiv: prefix and year
[He 2018 - arxiv:1712.00636]        # ASCII hyphen, must be em-dash
```

## When to cite

- Non-obvious technical decision drawn from prior work.
- Specific number lifted from a paper / repo (speedups, throughput, recall).
- Architectural pattern attributed to a paper (e.g. "two-process split [Flash-VStream — arxiv:2406.08085]").

When **not** to cite:

- Standard CS knowledge ("we use a hash map").
- Widely-known engineering ("RTSP is TCP-based").
- Implementation choices with no paper antecedent.

## When citing a number

The number must appear in the abstract or the cited section / table of the source. The `cite-checker` agent fetches the arxiv abstract and searches for the number; if not found, you must either expand the citation to point to the table, or rephrase qualitatively.

> Cite "56× speedup [Wang 2025 — arxiv:2503.13724]" only if the abstract or a labeled table says 56×. If the abstract says "up to 50×" and you wrote 56× from the paper body, link the paper note (`docs/02.papers/2503.13724-...md`) and quote the exact line.

## Paper notes

When a citation is load-bearing (multiple references in the repo, or a number that anchors a roadmap decision), write `docs/02.papers/<arxiv-id>-<short-slug>.md`:

```markdown
# <arxiv-id> — <Title>

## tldr

One paragraph.

## method

Two paragraphs max.

## numbers

The specific figures we cite. Quote the source line.

## why it matters here

How fovea uses this. What it would take to be wrong.
```

## We are not researchers

fovea is engineering OSS. We cite when it strengthens a technical decision, not to pad a paper. Drop a citation rather than ship a wrong one.

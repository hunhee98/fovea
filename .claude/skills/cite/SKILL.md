---
name: cite
description: Add or verify citations for non-trivial claims
---

Before merging any non-obvious technical claim:

1. Identify the claim and the prior work it leans on.
2. Open the cited paper / repo:
   - Verify the arxiv ID resolves to a real paper at https://arxiv.org/abs/<id>.
   - Verify the specific number being cited is actually in the abstract / table / repo.
3. If the claim is in the README / commit message:
   - Inline form: `[Author Year — arxiv:NNNN.NNNNN]` or `[Author Year — github.com/org/repo]`.
4. If the claim warrants a paper note:
   - Create `docs/02.papers/<arxiv-id>-<slug>.md` with: tldr, method, numbers, why-it-matters-here.
5. If the cited number cannot be verified from a published source: drop it or rephrase as "fast" / "efficient" without a number.

Hallucinated arxiv IDs are the biggest risk. Always click through. We'd rather drop a citation than ship a wrong number.

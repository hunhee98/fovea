---
name: feature
description: Plan and implement a new feature end-to-end
---

For new functionality the user describes in natural language.

1. **Plan first.** Lay out the steps in chat (don't necessarily persist). Ask at most one clarifying question, then read code to fill the rest. Surface non-obvious trade-offs (perf, license, dependency cost) before coding.
2. **Implement step-by-step**, verifying after each step:
   - Build green.
   - New unit / integration test exercises the change.
   - Existing tests still pass.
   - clippy clean.
3. **Surface measured numbers.** If the feature affects perf / accuracy / cost, run a quick measurement and quote the result in the commit body.
4. **End with `/verify` then `/done`.**

Cite prior work when a non-obvious technical decision rests on it. Inline citation form: `[Author Year — arxiv:NNNN.NNNNN]`.

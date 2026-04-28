---
name: fix
description: Diagnose and fix a bug or simple correction
---

For broken behavior the user describes.

1. **Reproduce.** Read the code path the user is referring to. Run the failing test / command. Get a concrete error message before changing anything.
2. **Root cause.** Don't patch symptoms. If the cause is upstream of the obvious failure point, follow it there.
3. **Smallest correct change.** No surrounding cleanup, no refactor "while we're here". A bug fix doesn't need to grow new abstractions.
4. **Add a regression test** that would have caught the bug.
5. `/verify` then `/done`.

If the bug is in a third-party library we vendor (e.g. `vendor/libde265/`), prefer a tightly-scoped patch in the vendored source over upstreaming first — vendoring is for surgical changes.

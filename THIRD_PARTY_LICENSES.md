# Third-party software included in this repository

| Component | Path | Upstream | License |
|-----------|------|----------|---------|
| libde265  | `vendor/libde265/` | https://github.com/strukturag/libde265 (commit `3cd9fbf`, vendored 2026-04-28) | **LGPL-3.0-or-later** (sample apps under MIT) |

## libde265

`vendor/libde265/` ships a near-verbatim copy of struktur AG's libde265
HEVC decoder. The two additions made on top of upstream are:

- `vendor/libde265/libde265/de265_internals.h`
- `vendor/libde265/libde265/de265_internals.cc`
- a small public-method addition inside `vendor/libde265/libde265/image.h`
- one extra source/header registration in `vendor/libde265/libde265/CMakeLists.txt`

These additions expose prediction-block motion-vector data that libde265
already computes internally, adapted from Christian Feldmann's analyzer
fork (https://github.com/ChristianFeldmann/libde265, branch `internal`,
commit `26b3b25`, 2018-10-07). Both upstream and the additions are
covered by LGPL-3.0-or-later. Fovea consumes libde265 by **dynamic
linking only** (`cargo:rustc-link-lib=dylib=de265`). Users redistributing
binary builds must ship libde265 source (or a written offer for it) per
LGPL §4. This repository's vendored source satisfies the source-shipping
obligation.

The rest of fovea-mv (Rust crates, Python package, examples) is licensed
Apache-2.0 — see `LICENSE`.

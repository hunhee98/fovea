//! Build glue for the libde265 + de265_internals C API used by the
//! HEVC code path.
//!
//! Strategy:
//! - Vendored libde265 source lives at `<repo>/vendor/libde265`. It must
//!   be built once via cmake (see `vendor/libde265/build/`).
//! - We tell rustc to link against the produced `libde265.dylib` (or
//!   `.so` on Linux) and embed an rpath so the binary can find it at
//!   runtime without `DYLD_LIBRARY_PATH` shenanigans.
//! - bindgen generates Rust bindings from `de265.h` and our extension
//!   header `de265_internals.h`.
//!
//! Override paths via env vars if the user has libde265 installed
//! system-wide:
//!   FOVEA_LIBDE265_DIR    — root containing libde265/de265.h headers
//!   FOVEA_LIBDE265_BUILD  — directory containing libde265.dylib

use std::env;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    // <repo>/crates/fovea-trigger-stream → <repo>
    let repo_root = manifest_dir.parent().unwrap().parent().unwrap();

    let libde265_src = env::var("FOVEA_LIBDE265_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| repo_root.join("vendor/libde265"));
    let libde265_build = env::var("FOVEA_LIBDE265_BUILD")
        .map(PathBuf::from)
        .unwrap_or_else(|_| repo_root.join("vendor/libde265/build/libde265"));

    if !libde265_src.exists() {
        panic!(
            "vendored libde265 source not found at {libde265_src:?} — \
             see docs/05.exec-plans/00X (HEVC) for the recipe"
        );
    }
    if !libde265_build.exists() {
        panic!(
            "libde265 build artifacts not found at {libde265_build:?} — \
             run `cmake -B vendor/libde265/build vendor/libde265 && \
             make -C vendor/libde265/build -j` first"
        );
    }

    println!("cargo:rerun-if-changed={}/libde265/de265_internals.h", libde265_src.display());
    println!("cargo:rerun-if-changed={}/libde265/de265.h", libde265_src.display());
    println!("cargo:rerun-if-env-changed=FOVEA_LIBDE265_DIR");
    println!("cargo:rerun-if-env-changed=FOVEA_LIBDE265_BUILD");

    println!("cargo:rustc-link-search=native={}", libde265_build.display());
    println!("cargo:rustc-link-lib=dylib=de265");
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", libde265_build.display());

    // bindgen — only header sets we use. Keep the surface tight so we
    // don't drag in unrelated decoder internals.
    let header_text = r#"
#include <libde265/de265.h>
#include <libde265/de265_internals.h>
"#;
    let bindings = bindgen::Builder::default()
        .header_contents("libde265-bindings.h", header_text)
        .clang_arg(format!("-I{}", libde265_src.display()))
        .clang_arg(format!("-I{}/build", libde265_src.display()))
        .allowlist_function("de265_.*")
        .allowlist_type("de265_.*")
        .allowlist_var("DE265_.*")
        .layout_tests(false)
        .generate_comments(false)
        .generate()
        .expect("bindgen failed for libde265");

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_dir.join("libde265_bindings.rs"))
        .expect("write libde265 bindings");
}

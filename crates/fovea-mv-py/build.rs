//! Embed the libde265 rpath into the Python extension's cdylib.
//!
//! `fovea-mv-stream` already adds an rpath via its own `build.rs`, but
//! Cargo's `rustc-link-arg` directives don't propagate to dependent
//! crates, so the cdylib produced for the Python extension would otherwise
//! end up with no rpath and fail at import time with "Library not loaded".

use std::env;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    // <repo>/crates/fovea-mv-py → <repo>
    let repo_root = manifest_dir.parent().unwrap().parent().unwrap();

    let libde265_build = env::var("FOVEA_LIBDE265_BUILD")
        .map(PathBuf::from)
        .unwrap_or_else(|_| repo_root.join("vendor/libde265/build/libde265"));

    println!("cargo:rerun-if-env-changed=FOVEA_LIBDE265_BUILD");
    println!("cargo:rustc-link-arg=-Wl,-rpath,{}", libde265_build.display());
}

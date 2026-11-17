//! Declares Beak's C ABI from its header, and links the library that keeps it: the beak-c crate where the workspace
//! builds it, else a distribution of Beak's — `include/beak.h` and `lib/<target>/libbeak_c.a` — at `BEAK_DIST`, or the
//! one this crate was distributed in, two folders up.

use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("cargo names the crate's directory"));
    println!("cargo:rerun-if-env-changed=BEAK_DIST");
    let header = if std::env::var_os("CARGO_FEATURE_PREBUILT").is_some() {
        let dist = std::env::var_os("BEAK_DIST").map_or_else(|| manifest.join("../.."), PathBuf::from);
        let target = std::env::var("TARGET").expect("cargo names the target");
        let lib = dist.join("lib").join(&target);
        assert!(
            lib.join("libbeak_c.a").is_file(),
            "no libbeak_c.a for {target} in {}",
            lib.display()
        );
        println!("cargo:rerun-if-changed={}", lib.join("libbeak_c.a").display());
        println!("cargo:rustc-link-search=native={}", lib.display());
        println!("cargo:rustc-link-lib=static=beak_c");
        dist.join("include/beak.h")
    } else {
        manifest.join("../beak-c/include/beak.h")
    };
    println!("cargo:rerun-if-changed={}", header.display());
    let bindings = bindgen::Builder::default()
        .header(header.to_str().expect("the header's path is UTF-8"))
        // As C23 reads it, a tagged union's tag is an enum of one byte.
        .clang_arg("-std=c2x")
        .allowlist_item("beak_.*|BEAK_.*")
        .default_enum_style(bindgen::EnumVariation::Rust { non_exhaustive: false })
        .derive_debug(true)
        .derive_default(true)
        .generate()
        .expect("the header reads");
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("cargo gives an out directory"));
    bindings.write_to_file(out.join("beak.rs")).expect("the declarations are written");
}

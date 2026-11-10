//! Compiles QuickJS — quickjs-ng, as `vendor/quickjs-ng` holds its library's sources — and the engine Siphon runs its
//! scripts on over it, `src/engine.c`, into the crate's static libraries.

use std::path::Path;

fn main() {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("cargo names the crate's directory");
    let quickjs = Path::new(&manifest).join("../../vendor/quickjs-ng");
    let engine = Path::new(&manifest).join("src/engine.c");
    let sources = ["dtoa.c", "libregexp.c", "libunicode.c", "quickjs.c"].map(|file| quickjs.join(file));
    for file in sources.iter().chain([&engine]) {
        println!("cargo:rerun-if-changed={}", file.display());
    }
    let build = || {
        let mut build = cc::Build::new();
        build
            .include(&quickjs)
            .std("c11")
            .define("_GNU_SOURCE", None)
            .flag_if_supported("-funsigned-char");
        build
    };
    // The library is built as its own build builds it; its warnings are its own.
    build().files(&sources).warnings(false).compile("quickjs");
    build()
        .file(&engine)
        .warnings(true)
        .extra_warnings(true)
        .warnings_into_errors(true)
        .compile("engine");
}

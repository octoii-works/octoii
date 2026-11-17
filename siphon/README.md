# Siphon workspace

This directory is the local staging layout for the future private `devxiyang/octoii-siphon` repository.
It is still tracked in the Octoii Git repository; no visibility or licensing change has occurred.

- `crates/`: Siphon, runtime, script, shell, text and image crates, with their existing names and behavior.
- `vendor/beak/`: Rust bindings, C headers and the existing prebuilt kernel distribution.
- `vendor/quickjs-ng/`: the sources compiled by siphon-script, with their license intact.
- `Cargo.lock`, `rust-toolchain.toml`, `rustfmt.toml`, `clippy.toml`: independent build inputs.

From this directory:

```sh
cargo check --locked --workspace --all-targets
cargo fmt --all --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
```

The product workspace consumes these crates by path. Siphon temporarily depends on `../libs/octoii-ai`,
which remains public product-workspace source. Before remote extraction, give that dependency an independently
obtainable source; copying this directory alone is not yet a standalone distribution.
The current Beak distribution only includes aarch64-apple-darwin. Other targets need matching artifacts.

The workspace split does not resolve Node's direct runtime/Beak access or separate Octoii's Agent implementation.
Those remain explicitly tracked in `../docs/SIPHON_BOUNDARIES.md`. Do not treat relocation as approval to change
kernel semantics or bypass the existing separately confirmed core-change process.

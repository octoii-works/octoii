# Siphon workspace

This directory is the local staging layout for the future private `devxiyang/octoii-siphon` repository.
It is still tracked in the Octoii Git repository; no visibility or licensing change has occurred.

- `crates/`: runtime, script, shell, text and image crates, with their existing names and behavior.
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

The product workspace consumes these crates by path. This workspace has no AI or product dependency.
The Agent host lives in the public `../libs/octoii` package.
The current Beak distribution only includes aarch64-apple-darwin. Other targets need matching artifacts.

The workspace split does not resolve Node's direct runtime/Beak access or finish separating the machine, storage and authorization mechanisms from the public Agent host.
Those remain explicitly tracked in `../docs/SIPHON_BOUNDARIES.md`. Do not treat relocation as approval to change
kernel semantics or bypass the existing separately confirmed core-change process.

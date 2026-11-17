# Superseded: Siphon → Beak submodule migration

2026-11-18. User requested that octoii-siphon depend on Beak through a Git submodule.
The submodule trial was approved and begun, but was not committed or pushed. The user subsequently
chose source integration as part of the binary interface goal. The replacement plan is
[SIPHON_BINARY.md](SIPHON_BINARY.md); this document is historical context, not the current execution plan.

## Target

Keep vendor/beak as the path, pointing to private devxiyang/octoii-beak at
19d94a80ce10b7f37a65c03b521ead46dace32c1. Use Beak's existing default workspace feature to build
its C ABI implementation from source. Do not maintain copied bindings, headers or libbeak_c.a in Siphon.

The existing C header and safe Rust binding source match this commit. The beak-sys source difference
is its workspace-feature link to beak-c; manifest differences select source versus prebuilt mode.
The separate local Beak checkout has an uncommitted toolchain change, which this migration must not touch.
Pin the committed remote revision, and build it under Siphon's existing Rust 1.100 toolchain.

## Exact core scope

In octoii-siphon:

- Replace the 14 files listed below with a vendor/beak gitlink; add .gitmodules.
- Cargo.toml: remove vendor/beak/crates/* from members and exclude vendor/beak so Beak keeps its own workspace.
  Keep the beak path dependency; its existing default feature selects source builds.
- Cargo.lock: resolve the source-built Beak crates and their dependencies.
- README.md and .github/workflows/check.yml: document and fetch the pinned nested dependency.

No changes to Siphon runtime, scripts, machine behavior, Rust tests or Beak implementation files.
Any source changes found necessary must receive a separate scope confirmation.
An extension-only change cannot replace the tracked kernel distribution or correct workspace membership.

In the product repository: update the Siphon pin, Cargo.lock, dependency checks to include the Beak
workspace, CI checkout and documentation. Existing dependency paths remain unchanged.
CI needs a dedicated read-only Beak deploy key: the Siphon-only key cannot read another private repository.
Use the Beak read-only key as an encrypted secret in Siphon and the product; no private keys in source.

## Validation and release

- Full recursive checkout retrieves both private dependencies at fixed commits.
- Metadata shows separate product, Siphon and Beak workspaces and no AI/product dependency from the kernel.
- Build from source with no BEAK_DIST override or old bundled libbeak_c.a.
- Formatting, Clippy and relevant existing suites pass; Node builds and existing product regressions pass.
- Siphon CI verifies the nested checkout and source build.
- Commit and push Siphon first, then update and push the product gitlink. Existing 0.0.1 tags remain unchanged.
- This is build organization work, not a claim that Windows/Linux binary distribution is already implemented.

## Replaced tracked files (relative to octoii-siphon)

```text
vendor/beak/crates/beak-sys/Cargo.toml
vendor/beak/crates/beak-sys/build.rs
vendor/beak/crates/beak-sys/src/lib.rs
vendor/beak/crates/beak/Cargo.toml
vendor/beak/crates/beak/src/command.rs
vendor/beak/crates/beak/src/ffi.rs
vendor/beak/crates/beak/src/kernel.rs
vendor/beak/crates/beak/src/layout.rs
vendor/beak/crates/beak/src/lib.rs
vendor/beak/crates/beak/src/namespace.rs
vendor/beak/crates/beak/src/queue.rs
vendor/beak/crates/beak/src/snapshot.rs
vendor/beak/include/beak.h
vendor/beak/lib/aarch64-apple-darwin/libbeak_c.a
```

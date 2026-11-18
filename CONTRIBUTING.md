# Contributing

Thanks for wanting to help. This repository takes contributions in a set order, so a maintainer can give each the time
it needs.

## Issues

- **New contributors' issues close on their own.** Maintainers read the closed ones daily and reopen those worth
  working on. A maintainer who replies `octoii:allow-issues` keeps your later issues open; `octoii:allow-prs` keeps your
  later issues and pull requests open.
- **What gets reopened:** one problem per issue, in your own words, short enough to read on one screen. A bug says what
  happened, the smallest steps that make it happen, and what you expected; a proposal says what to change and why.
  Search the open and closed issues first.
- **Security problems** are not issues: report them privately to the maintainers through GitHub's security advisories.

## Pull requests

- **Propose first.** Open a Contribution Proposal issue; a pull request from a contributor not approved with
  `octoii:allow-prs` closes on its own.
- **Checks pass.** `bun install`, then `bun run check`: formatting, lint, unused code, types, and the tests on Bun and
  on Node.
- **Rust changes:** run `python3 tools/check-rust-layers.py`, `cargo fmt --all --check`,
  `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` on a platform supported by the
  bundled Beak library. Follow the [package boundaries](docs/ARCHITECTURE.md).
- **Keep it to one change**, written like the code around it, within the rules of [AGENTS.md](AGENTS.md): clear names,
  small functions, the simplest design that works, and nothing built before it is needed.

## Native build dependency

The Rust workspace links the released native library for its locked SDK version. Install that
library before building or testing Cargo targets from the repository.

```sh
export SIPHON_LIB_DIR="$(python3 tools/install-siphon.py)"
cargo build --locked -p octoii-node
```

## Local verification cleanup

Reuse Cargo's intermediate build cache. For an isolated verification run, keep `CARGO_BUILD_BUILD_DIR` stable
for each workspace and put `CARGO_TARGET_DIR`, test data and native link outputs in a temporary directory.
From the Octoii checkout root, for example:

```sh
(
  run_dir="$(mktemp -d)" || exit 1
  trap 'rm -rf -- "$run_dir"' EXIT
  export SIPHON_LIB_DIR="$(python3 tools/install-siphon.py)"
  export CARGO_BUILD_BUILD_DIR="$PWD/target/local-cache/{workspace-path-hash}"
  export CARGO_TARGET_DIR="$run_dir/target"
  export TMPDIR="$run_dir"
  cargo test --locked -p octoii
)
```

Stop processes started for verification and remove only the run's temporary outputs, even on failure.
Keep the reuse directory and Cargo's dependency cache. Normal development builds may keep their default
`target/`; do not run `cargo clean` as routine teardown. If space is scarce, inspect
`du -sh target/local-cache/*` and prune unused workspace caches with no build running.

[Cargo 1.100](https://blog.rust-lang.org/2026/11/12/Rust-1.100.0/#reducing-cargo-cache-collisions)
improves cache isolation; it does not provide automatic cross-workspace deduplication or eviction of build
outputs. For frequently rebuilt worktrees, an opt-in [sccache](https://doc.rust-lang.org/stable/cargo/reference/build-cache.html#shared-cache)
can share eligible compiler results across workspaces. Its local cache has a
[configurable size limit](https://github.com/mozilla/sccache/blob/main/docs/Local.md); it complements rather
than replaces the Cargo build directory.

# Source repositories and reproducible builds

2026-11-18. The product consumes public octoii-ai 0.0.2 from crates.io. Only Siphon remains a submodule.

| Dependency | Source | Product integration |
| --- | --- | --- |
| Siphon | [devxiyang/octoii-siphon](https://github.com/devxiyang/octoii-siphon), private | Fixed by the `siphon/` gitlink in this checkout |
| octoii-ai | [crates.io/octoii-ai](https://crates.io/crates/octoii-ai/0.0.2), public MIT package | Cargo version 0.0.2; resolved source and checksum in Cargo.lock |
| Siphon binary interface | [octoii-works/octoii-siphon-lib](https://github.com/octoii-works/octoii-siphon-lib), public | Development wrapper tested independently against the native archive; not yet a product dependency |

AI source and independent CI live in [octoii-works/octoii-ai](https://github.com/octoii-works/octoii-ai).
Its 0.0.2 tag names release commit 9b3bcec729b83609a1783c381bf2c269b239a65b. The original 0.0.1 tag is unchanged.
There is no AI source directory or AI submodule in this product checkout. AI's public Rust import is octoii_ai.

Product code owns libs/octoii, apps and extensions. Siphon has no AI/product dependency. Repository separation
still leaves controlled storage, machine/sandbox and authorization execution to converge. Existing source
licenses and third-party notices are retained; earlier public Git history is not rewritten.

## Clone and build

Rust builds currently require read access to private Siphon. Authenticate HTTPS Git access with an account
that can read it (for GitHub CLI users: gh auth login, then gh auth setup-git).

```sh
git clone --branch codex/mvp --recurse-submodules https://github.com/octoii-works/octoii.git
cd octoii
git submodule update --init --recursive
cargo build --locked -p octoii-node --bin octoii
```

The integration remains on codex/mvp until merged into main. Submodule updates use the recorded commit,
not --remote branch tips. Cargo downloads octoii-ai from crates.io without GitHub credentials.
Beak is built from source in siphon/beak, an independent workspace copied into the private repository.
No nested Beak checkout or additional repository credential is required. Platform validation currently targets macOS.

Validate the three local workspaces:

```sh
python3 tools/check-rust-layers.py
cargo test --locked --workspace
cargo test --locked --manifest-path siphon/Cargo.toml --workspace
cargo test --locked --manifest-path siphon/beak/Cargo.toml --workspace
```

Run fmt and Clippy with those same manifests. AI's own tests run in its separate public repository;
product tests exercise the published library through Octoii and its extensions.

## Update dependencies

For AI, publish the tested version from its repository first, then update the product Cargo dependency
and lockfile and validate the product. Do not introduce a local path or Git override for release builds.
For Siphon, push its commit first, check out that exact commit in the submodule, validate the product,
and commit the gitlink update. Never pin a commit unavailable from the remote.

## CI access

The product Rust job reads the Siphon pin from Git and checks it out with SIPHON_DEPLOY_KEY, an Actions
secret paired with a read-only deploy key on devxiyang/octoii-siphon. Checkout disables credential
persistence. The temporary local key was removed after testing access. No key material is committed.
AI is resolved normally by Cargo; the product no longer checks out or runs a third AI workspace.

Fork pull requests cannot run the product Rust job because they lack the private dependency;
frontend jobs remain independent. Each dependency repository maintains its own CI. Revoking the
GitHub CLI authorization used to create the deployment key may remove that key; rotate it together
with the Actions secret when necessary.

## Next distribution boundary

The agreed target is a public octoii-works/octoii-siphon-lib repository and published library named
octoii-siphon-lib, containing the safe Rust interface wrapper, ABI declarations, documentation and
versioned platform binaries accessible without private repository credentials. The private repository would retain
implementation source. [Binary execution and recovery slices](SIPHON_EXECUTION.md) are underway;
convergence of the remaining Beak/runtime access is a prerequisite. The product still requires
private Siphon source today. Final acceptance requires building and running Octoii without private
source access; Octoii must no longer directly depend on Beak, siphon-runtime or the Siphon source submodule.

macOS, Windows and Linux binary support is a required release goal, not an optional later claim.
The implementation plan must explicitly define CPU architectures, Linux libc/ABI baselines and supported
OS versions, with artifacts and real execution, persistence/recovery, cancellation and permission tests
for each supported target. Native binary validation is currently macOS ARM64; additional artifacts and
platform implementations must be established before their targets are supported.

## Public AI package validation

For 0.0.2, standalone AI tests (39 passed / 1 existing ignored), Clippy and cargo publish --dry-run
passed before publication. The published package was downloaded from crates.io and Cargo metadata
confirmed the registry source. Product checks, Clippy, 223 tests (2 existing ignored) and Node build
passed with no local AI checkout. Cargo.lock adds the registry source and checksum without changing
other dependency versions. The independent AI CI passed for release commit 9b3bcec.

## Extraction validation

Local checks and Clippy passed. Product tests: 223 passed / 2 existing ignored; standalone AI:
39 passed / 1 existing ignored; standalone Siphon: 45 passed. Node builds with the pinned sources.
The extracted sources were compared against the product revision: 76 existing Siphon files and
47 existing AI files are byte-identical; changes are standalone configuration and documentation.
AI's initial GitHub CI passed. Siphon's initial GitHub CI passed compilation and Clippy but exposed
an existing memory-limit test assumption: the exhausted engine reported Thrown instead of InternalError.
The separately approved correction in Siphon commit 91272f465dd2ee48b969eb41230571f65457fb9f uses
one finite allocation workload: it must fail at a 4 MiB limit and return the full count at 64 MiB.
The product pinned this follow-up commit before the binary work. All 9 script tests and targeted Clippy passed locally;
production code is unchanged. The full standalone GitHub CI passed on this follow-up commit
([run 44114764949](https://github.com/devxiyang/octoii-siphon/actions/runs/44114764949)).

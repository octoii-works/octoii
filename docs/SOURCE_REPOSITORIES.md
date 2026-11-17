# Source repositories and reproducible builds

2026-11-17. The user approved extracting the existing sources into independent repositories,
with pinned submodules at their existing product paths. This moves repositories before completing
A2's mechanism boundary work; that work remains tracked in SIPHON_EXTRACTION.md.

| Repository | Access | Product path | Initial pinned commit |
| --- | --- | --- | --- |
| [devxiyang/octoii-siphon](https://github.com/devxiyang/octoii-siphon) | Private | siphon/ | 8b4ae3b06f78ca6f941dfdd9c4c6a264f4bf7667 |
| [octoii-works/octoii-ai](https://github.com/octoii-works/octoii-ai) | Public, existing MIT license | libs/octoii-ai/ | d6742fdbe6eb56c07ba6665efa204dbfb7a5cd42 |

The Git index pins commits, not branch tips. Cargo still uses the existing paths. Product code owns
libs/octoii, apps and extensions; AI and Siphon each own a standalone Cargo workspace and lockfile.
Siphon has no AI/product dependency. Repository separation does not finish the controlled-storage,
machine/sandbox and authorization execution boundaries. Existing source licenses and third-party
notices are retained; earlier public Git history is not removed or rewritten.

## Clone and build

Rust builds require access to the private Siphon repository. Authenticate HTTPS Git access with an
account that can read it (for GitHub CLI users: gh auth login, then gh auth setup-git).

```sh
git clone --branch codex/mvp --recurse-submodules https://github.com/octoii-works/octoii.git
cd octoii
# After switching to a revision containing these submodules:
git submodule update --init --recursive
cargo build --locked -p octoii-node --bin octoii
```

The initial integration is on codex/mvp; use that branch until merged into main. Existing checkouts
should update submodules after switching revisions. Do not use --remote for normal builds: it selects
branch tips rather than the product's tested commits. The bundled Beak artifact currently supports
aarch64-apple-darwin; source separation does not introduce additional binary targets.

Validate all three workspaces:

```sh
python3 tools/check-rust-layers.py
cargo test --locked --workspace
cargo test --locked --manifest-path libs/octoii-ai/Cargo.toml --workspace
cargo test --locked --manifest-path siphon/Cargo.toml --workspace
```

Run fmt and Clippy with the same three manifests. Product --workspace no longer runs AI's own tests.
Live-model tests remain opt-in; normal tests do not require model credentials.

## Update a dependency

Commit and push dependency changes in its owning repository first. Then check out that exact commit
in the corresponding product submodule, validate the product, and commit the gitlink update in Octoii.
If dependencies changed, update the relevant lockfiles as well. Never commit a product gitlink that
points to a dependency commit unavailable from its remote.

## CI access

The product Rust job reads both pins from Git, then checks out each repository at that exact SHA.
Public AI needs no additional credential. Private Siphon uses SIPHON_DEPLOY_KEY in the product's
Actions secrets, paired with a read-only deploy key on devxiyang/octoii-siphon. The key grants no push
access. Checkout disables credential persistence. No key material is committed; the temporary local
copy used for setup is removed after testing access.

Fork pull requests do not run the product Rust job because they cannot access the private dependency;
frontend jobs remain independent. Each extracted repository also has its own check workflow. The
product build still requires private source access; binary-only distribution is a separate task.

The deployment key was added using the current GitHub CLI authorization. If that authorization is
revoked, GitHub may remove its associated deploy key; rotate the key and Actions secret together.

## Extraction validation

Local checks and Clippy passed. Product tests: 223 passed / 2 existing ignored; standalone AI:
39 passed / 1 existing ignored; standalone Siphon: 45 passed. Node builds with the pinned sources.
The extracted sources were compared against the product revision: 76 existing Siphon files and
47 existing AI files are byte-identical; changes are standalone configuration and documentation.
AI's initial GitHub CI passed. Siphon's initial GitHub CI passed compilation and Clippy but exposed
an existing memory-limit test assumption: the exhausted engine reported Thrown instead of InternalError.
A separately scoped test correction was requested; production code was not changed to mask it.

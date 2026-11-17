# Package boundaries

Three Cargo workspaces separate the product, public AI library and private Siphon repository. The root owns apps, extensions
and libs/octoii; libs/octoii-ai/ and siphon/ are pinned submodules with independent workspaces. Runtime behavior remains unchanged.

| Location | Responsibility |
| --- | --- |
| `siphon/vendor/beak/` | Beak's Rust bindings, C header and prebuilt kernel distribution |
| `siphon/crates/siphon-runtime/` | Durable execution, requests and recovery on Beak; no AI dependency |
| `libs/octoii-ai/` | Model vocabulary, providers and transports |
| `siphon/crates/siphon-{script,shell,text,image}/` | Foundational processing libraries |
| `libs/octoii/` | Agent, threads, basic tools, permissions, protocol and public extension interface |
| `extensions/` | Built-in backend capabilities and integrations; shared extension helpers |
| `apps/node/` | The `octoii-node` Cargo package (binary `octoii`): node, CLI, authentication, pairing and extension assembly |
| `packages/` | TypeScript client and reusable frontend packages |
| `apps/web/`, `apps/desktop/` | Product interface and Electron host |

## Rust dependencies

Each workspace declares its own package defaults, lints, profiles and dependencies, and keeps its own Cargo.lock.
The product uses path dependencies into siphon/; Siphon has no dependency on AI or the product. Beak and QuickJS
distribution files live inside siphon/vendor. AI has no dependency on Siphon or the product.

The repositories are extracted and pinned. Controlled storage, machine and authorization mechanisms still need
boundary convergence. Existing licensing is retained. See [SOURCE_REPOSITORIES.md](SOURCE_REPOSITORIES.md).

- Kernel packages depend only on other kernel packages within this workspace.
- AI must not depend on private implementation packages. Foundational processing libraries may depend on one another, never on AI, runtime, Octoii, extensions or apps.
- `siphon-runtime` may depend on the kernel, never on AI or the product.
- `octoii` may depend on core libraries and the kernel, never on concrete extensions or apps.
- Extensions may depend on Octoii's public host interface, `octoii-ai`, and other extension packages. Cargo rejects cycles.
  They must not depend directly on Beak, `siphon-runtime`, or other core implementation libraries. Shared helpers such
  as `siphon-html` are libraries, not necessarily independently enabled extensions.
- Apps assemble core and extensions. Libraries never depend on an app; apps do not depend on other apps.

An extension reaches the volume and execution machinery through the existing public primitives, now hosted in Octoii. The node is the trusted
composition root and still has lower-layer dependencies for initialization, files and access records; these are tracked in
[SIPHON_BOUNDARIES.md](SIPHON_BOUNDARIES.md), not permission to expand access.

Run `python3 tools/check-rust-layers.py` to validate these rules. CI runs it before Rust compilation. The check uses
`cargo metadata --no-deps --locked --offline`, combines members from all three workspaces, reads actual local dependency identities rather than aliases or manifest
text, and includes normal, build, dev, optional and platform-specific dependencies. New core packages require an
explicit classification. Cargo enforces dependency resolution and cycles; this check additionally enforces direction.

## Frontend boundary

UI packages stay separate from Rust extensions. Product UI packages consume the client and shared UI contracts, never
application internals. Apps assemble them. A frontend feature may consume several backend capabilities; frontend and
backend packages need not match one-to-one.

The current web app still contains product UI modules under `apps/web/src/extensions` and shared components. Independent UI packages are deferred; keep clear module boundaries inside the web app. This directory migration does
not introduce a dynamic plugin loader or change execution behavior.

## Proposed source-distribution boundary

The proposed closed Siphon implementation and public Octoii/extension boundary, including current coupling and
binary distribution prerequisites, is documented in [SIPHON_DISTRIBUTION.md](SIPHON_DISTRIBUTION.md).
Remote extraction is implemented with pinned submodules; interface convergence remains separate work.
Moved kernel files retain the existing core-stability review intent; moving directories is not permission to change
runtime behavior without the separately scoped confirmation. AGENTS.md is unchanged.

## Validation

Run cargo fmt, clippy and test for the root manifest, `siphon/Cargo.toml` and `libs/octoii-ai/Cargo.toml`;
root --workspace selects neither Siphon nor AI tests. CI checks all three. `cargo build -p octoii-node --bin octoii --locked` remains the product build command.
Siphon has its own toolchain/format/lint configuration and target directory. All three workspaces currently pin Rust 1.100.0.

Local extraction validation (2026-11-17): Cargo resolves 16 product members and 8 Siphon members; both locked
workspaces pass all-target checks, formatting and Clippy. Siphon tests: 153 passed / 1 existing ignored;
product tests: 154 passed / 2 existing ignored. Migration preserved 188 implementation/header/binary files byte
for byte and introduced no dependency-version changes. Two existing Node tests assumed eight access scopes;
the current nine-scope policy is unchanged, and the tests now compare the complete declared scope set.

A1 migration: `Octoii` is the public Agent host type. The runtime remains `siphon-runtime`; no empty
Siphon facade is introduced. Existing low-level access moved with the host is tracked debt, not new access
authorization. See [SIPHON_EXTRACTION.md](SIPHON_EXTRACTION.md) for scope and follow-up gates.

A1 validation (2026-11-17): 17 product members and 7 private-workspace members; checks and Clippy pass.
Product tests: 262 passed / 3 existing ignored; private tests: 45 passed. Generated protocol declarations
are unchanged (four documentation comments rename the host). No new third-party dependency versions.

# Package boundaries

One Cargo workspace owns the Rust packages. `core/` is a directory of independent crates, not a crate or a nested
workspace. Package names and the `octoii` executable stay independent of their directory names.

| Location | Responsibility |
| --- | --- |
| `vendor/beak/` | Beak's Rust bindings, C header and prebuilt kernel distribution |
| `core/siphon-runtime/` | Durable execution, requests and recovery on Beak; no AI dependency |
| `core/octoii-ai/` | Model vocabulary, providers and transports |
| `core/siphon-{script,shell,text,image}/` | Foundational processing libraries |
| `core/siphon/` | Agent, threads, basic tools, permissions, protocol and public extension interface |
| `extensions/` | Built-in backend capabilities and integrations; shared extension helpers |
| `apps/node/` | The `octoii` Cargo package: node, CLI, authentication, pairing and extension assembly |
| `packages/` | TypeScript client and reusable frontend packages |
| `apps/web/`, `apps/desktop/` | Product interface and Electron host |

## Rust dependencies

The root `[workspace.dependencies]` declares paths to core and extension packages. Their consumers inherit these
declarations with `workspace = true`. Beak's distributed crates keep their own internal binding dependency.

- Kernel packages depend only on other kernel packages within this workspace.
- AI and foundational processing libraries may depend on one another, never on runtime, Siphon, extensions or apps.
- `siphon-runtime` may depend on the kernel, never on AI or the product.
- `siphon` may depend on core libraries and the kernel, never on concrete extensions or apps.
- Extensions may depend on Siphon's public interface, `octoii-ai`, and other extension packages. Cargo rejects cycles.
  They must not depend directly on Beak, `siphon-runtime`, or other core implementation libraries. Shared helpers such
  as `siphon-html` are libraries, not necessarily independently enabled extensions.
- Apps assemble core and extensions. Libraries never depend on an app; apps do not depend on other apps.

An extension reaches the volume and execution machinery through Siphon's public primitives. The node is the trusted
composition root and may access lower layers to initialize them.

Run `python3 tools/check-rust-layers.py` to validate these rules. CI runs it before Rust compilation. The check uses
`cargo metadata --no-deps --locked --offline`, reads actual local dependency identities rather than aliases or manifest
text, and includes normal, build, dev, optional and platform-specific dependencies. New core packages require an
explicit classification. Cargo enforces dependency resolution and cycles; this check additionally enforces direction.

## Frontend boundary

UI packages stay separate from Rust extensions. Product UI packages consume the client and shared UI contracts, never
application internals. Apps assemble them. A frontend feature may consume several backend capabilities; frontend and
backend packages need not match one-to-one.

The current web app still contains product UI modules under `apps/web/src/extensions` and shared components. Independent UI packages are deferred; keep clear module boundaries inside the web app. This directory migration does
not introduce a dynamic plugin loader or change execution behavior.

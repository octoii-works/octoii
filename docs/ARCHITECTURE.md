# Package boundaries

The root Cargo workspace owns the Octoii host, extensions, apps and processing libraries.
It uses the crates.io packages `octoii-ai` and `octoii-siphon-lib`. The private Siphon repository
contains the runtime and its internal Beak workspace.

| Location | Responsibility |
| --- | --- |
| crates.io `octoii-siphon-lib` | Safe Rust interface to the released Siphon binary for storage, Fiber execution and machine operations |
| crates.io `octoii-ai` | Model vocabulary, providers and transports; no local source directory |
| `libs/siphon-{script,shell,text,image}/` | Octoii's script engine, shell parsing, text operations and image processing |
| `vendor/quickjs-ng/` | QuickJS sources built by `libs/siphon-script` |
| `core/` | The `octoii` Cargo package. Agent, threads, tools, permissions, protocol and the public extension interface |
| `extensions/` | Built-in backend capabilities and integrations; shared extension helpers |
| `apps/node/` | The `octoii-node` Cargo package (binary `octoii`): node, CLI, authentication, pairing and extension assembly |
| `packages/` | TypeScript client and reusable frontend packages |
| `apps/web/`, `apps/desktop/` | Product interface and Electron host |

## Rust dependencies

- Octoii uses `octoii-siphon-lib` for volume and execution. Extensions reach those capabilities through Octoii's
  public interface. Only Octoii directly depends on the SDK.
- Foundational processing libraries may depend on one another.
- Extensions may depend on Octoii's public host interface, `octoii-ai`, and other extension packages. Cargo rejects cycles.
  Neither extensions nor Octoii depend on private runtime or Beak source.
- Apps assemble Octoii and extensions. Libraries never depend on an app; apps do not depend on other apps.

Run `python3 tools/check-rust-layers.py` to validate the resolved dependency graph. The check includes build, test,
optional and target-specific dependencies.

`core/src/lib.rs` declares the public modules and exports. `host.rs` holds the running host and
`host/build.rs` opens the volume, assembles extensions and registers execution. Thread startup and its storage
rules live in `threads/start.rs`. The Agent owns the execution phases. Tool interfaces, request context and
registration live in `tools/{tool,context,registry}.rs`, with the same call runner used by turns and scripts.

`apps/node/src/http/` contains handlers used by the loopback and tunnel HTTP router. The local control socket uses
its own state for node status and access management.

## Node protocol

- `POST /api/v1/call` accepts a JSON object with `method` and `params`. The HTTP status accompanies either `result` or
  `error` with a code and message. The app sends the same session with each request. Initial task submission still
  uses `POST /threads` so it can carry attachments in the same request. File reads and uploads have their own HTTP routes.
- `/api/v1/watch` uses the `octoii.watch.1` WebSocket subprotocol. Clients send `subscribe` and `unsubscribe` controls.
  The node sends live events and binary frames for terminals and browser pages. Loopback and tunnel clients use the
  same routes and scopes.
- A Thread watch starts with `thread_snapshot`. It contains up to 500 requested history entries and the status,
  including pending approvals, from the same commit. `thread_update` carries appended entries and status with their
  commit sequence. Reconnecting or falling behind starts from a fresh view. Older entries are available via
  `threads.read`. Live model output follows its request identity and may reset.
- Device revocation ends active watches. Each new HTTP request, including an attachment read, checks the session.
  When the socket closes before its welcome, the client checks `GET /api/v1/session`. A valid session returns
  `204` and a revoked or expired session returns `401`. The response is empty and sets `Cache-Control: no-store`.
  Approval and stop commands carry the exact identity shown to the user. The client retries only commands that
  carry a key the node deduplicates.

## Frontend boundary

UI packages stay separate from Rust extensions. Product UI packages consume the client and shared UI contracts, never
application internals. Apps assemble them. A frontend feature may consume several backend capabilities; frontend and
backend packages need not match one-to-one.

The current web app still contains product UI modules under `apps/web/src/extensions` and shared components. Independent UI packages are deferred; keep clear module boundaries inside the web app. This directory migration does
not introduce a dynamic plugin loader or change execution behavior.

## Siphon boundary

Octoii pins Siphon SDK 0.0.4 and ABI 6. Install the matching native release as described in
[CONTRIBUTING.md](../CONTRIBUTING.md). The installer and SDK verify the native files before linking.
This build requires a current-format volume. Start it with a separate Octoii home when testing alongside an ABI 5 node.

Octoii opens the volume and registers extensions and provider state before starting Fiber execution. Thread records,
logs and files live in the volume's data paths. Each Thread's root Fiber has an independently derived execution
address. Starts, stops and deliveries use SDK Fiber identities, and Thread removal commits data and Fiber
deletion together. Client status subscriptions observe commit heads because execution-only changes may have no
data path in the journal.

Host operations that decide what to write from a snapshot commit the whole batch against that snapshot's head.
On conflict they read again and re-evaluate the operation. This keeps request receipts with their writes, pairing
link consumption with session creation, and resume replacement with removal of the earlier schedule.

Commands for an active tool request use that request's SDK capability. Host-owned terminals and service
processes use the host's machine interface.

## Validation

Run formatting, Clippy and workspace tests for the product's root manifest. The private Siphon and public AI
workspaces validate independently. `cargo build -p octoii-node --bin octoii --locked` builds the product node.

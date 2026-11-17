# Siphon host requests over the binary

[B2.2a](SIPHON_EXECUTION.md) runs programs through the binary. B2.2b lets a program make requests of a host's
asynchronous handlers. The existing runtime still owns request delivery, answer commits, stop, and recovery.
The public Rust crate owns typed payloads and schedules the handler on the host's executor.

## Interface

- Register a handler by request name on `Builder` before opening the runtime. The Rust wrapper keeps
  `Request`, `Asked`, `Answer`, and `Unanswered` typed on the host side. The host executor is captured
  or supplied at registration; a callback can run on a native thread before open returns, so it must
  never infer the host executor from the callback thread or wait for open to produce a runtime handle.
- `Run::request(&request)` stages JSON with the current phase and returns a Siphon-generated mailbox key.
  Keep it in the checkpoint. `Wait::mail(key)` uses the existing mailbox wait; `Run::answer::<R>(key)`
  reads the phase snapshot; `Run::take_mail(key)` stages removal with the phase's writes. A failed phase
  does not publish its request. The mailbox helpers use the phase's read-only context and the same staged
  changes as the other operations, keeping the entire phase in one ordered batch.
- The C registration has a synchronous delivery callback. It receives borrowed payload, process path
  and key, and ownership of one opaque reply handle. It copies what it needs, queues work and returns.
  It can return an optional per-delivery notification callback and context release function. The native
  waiter calls the notification at most once for stop, and once when the delivery closes, then releases
  its context. These callbacks only notify the host; they do not run or wait for a handler. They use the
  same blocking-call guard as Program callbacks. No Rust future, executor, or token crosses the C ABI.
- The reply handle has a single [one-shot sender](https://docs.rs/tokio/latest/tokio/sync/oneshot/index.html),
  with no runtime handle or path/key commit operation. A submission
  copies one of `Given` (JSON), `Failed` (UTF-8), or `Interrupted`. Success means it entered the native
  receiver, not that it committed. Siphon uses its existing answer task for durable admission and commit.
  Duplicate submissions, and submissions after the receiver closes, return a closed-delivery error.
  Releasing a pending handle while its native waiter is live is an interruption.

The new binary API chooses the request key. Beak's current answer guard checks the process's creation sequence
and the existence of a request at its key. It does not distinguish two requests posted with the same key within
the same live process. The existing volume allocator avoids that case: it generates a fresh key per staged
request and preserves ID progress with the volume commit. Existing source callers that supply keys keep their
current API. The binary API has no caller-supplied key on the request creation path.

Keep product identifiers such as a tool call's ID separate from this mailbox key. If a future kernel caller
needs an address that can be reused while an earlier delivery is pending, Beak should store a distinct delivery
ID and compare it in the same transaction that commits the answer. The runtime would then match it when tracking
cancellation too. This is a separate kernel design change with its own acceptance tests; B2.2b does not require it.

## Stop, close, and reopen

Stop notifies the host while leaving the reply handle usable. The handler may return a partial result, or
`Interrupted` if it has nothing to give. The native runtime decides whether the requesting process can still
accept an answer. The wrapper does not abort the host future just because stop was requested.

Close drops the native receiver before signaling that delivery has closed. The wrapper stops waiting for its
host future; the native library does not join host handler tasks. An outstanding handle can outlive close without
retaining the volume. On persistent reopen, with the handler name registered again, the existing retold-request
path records `Interrupted` for an unanswered request and does not dispatch the handler again. It does not promise
that work outside Siphon can be rolled back, or that a response accepted just before close was committed.

## Private core scope — confirmed

Paths are relative to `devxiyang/octoii-siphon`.

| Files | Change |
| --- | --- |
| `crates/siphon-runtime/src/lib.rs`, `crates/siphon-runtime/src/request.rs`, `crates/siphon-runtime/src/runtime/builder.rs` | Add an encoded host handler through the existing handler map and answer task. Share the answer-mail conversion with source handlers. |
| `crates/siphon-runtime/src/program.rs` | Share phase-bound request staging, answer reading and mail removal with the binary adapter. Generate the binary request key with the existing volume allocator and preserve the order of phase changes. |
| New `crates/siphon-ffi/src/request.rs`; existing `crates/siphon-ffi/src/builder.rs`, `crates/siphon-ffi/src/program.rs`, `crates/siphon-ffi/src/lib.rs`, `crates/siphon-ffi/include/siphon.h` | Register and hand off requests, own replies and notifications, expose phase ask/read/take and mail waits. Use development ABI 3 so a storage/program-only ABI 2 archive is rejected at build time. |
| New `crates/siphon-ffi/tests/request.rs`, `crates/siphon-ffi/tests/c/request.c`; existing `crates/siphon-ffi/tests/c/lifecycle.c`, `crates/siphon-ffi/tests/c/program.c` | Cover the ABI ownership and request contract, and update the old C version assertions. |
| `crates/siphon-ffi/Cargo.toml`, `Cargo.lock`, `README.md`, `.github/workflows/check.yml` | Add only channel/notification support needed by the bridge, update the artifact manifest and documentation, and test C and the exact public wrapper commit against the same archive. |

Beak, storage format, the dispatcher, native stop/recovery and the commit loop stay as they are. Extensions
cannot register handlers in the already compiled native library, or access the phase's scoped request and answer
primitives across ABI 2. That is why the private adapter and the shared runtime seam are needed.

In the public `octoii-siphon-lib` repository, add `src/request.rs` and `tests/request.rs`; update
`src/{lib,program,sys}.rs`, `include/siphon.h`, `build.rs`, Cargo dependencies/lockfile, and README.
Keep Tokio on its own side of the ABI. Machine operations, streaming, human answers, and the product dependency
switch remain separate.

## Acceptance

1. Link a standalone C program and a Rust consumer to the actual ABI 3 archive. Run two requests from the
   same process, get distinct keys, and commit the answers with their phase results. A duplicate response
   must be refused even while a later request is pending. A thrown phase that stages a request never
   delivers it.
2. Stop a waiting process after delivery has started. The handler observes cancellation and gives a partial
   answer; the stop phase reads it and can preserve it as its result.
3. Close while a handler is still pending. Close returns without awaiting the host handler. Hold its reply
   handle across persistent reopen; the process reads `Interrupted`, the handler is not re-invoked, and the
   old reply is refused. Check registration release on failed open, and per-delivery release on normal
   completion and abandonment. Public Rust contains panics in handler construction and polling.
4. Keep existing storage and Program tests, run native and independent consumer CI, and remove local build
   directories, binaries and test state after validation.

## Development verification

Private implementation `795bdb1`; public wrapper `ee0b752`. On macOS ARM64 with Rust 1.100.0, all three
standalone C consumers and all 13 independent Rust consumer tests passed against the ABI 3 archive.
The private FFI/runtime passed 22 tests, and formatting and strict Clippy passed for the changed crates.
Octoii's library still compiles with the private source pin, and its dependency boundary check passed.
This is a development slice; the native archive and Cargo package have not been released.

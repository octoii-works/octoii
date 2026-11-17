# Siphon binary execution

The [binary distribution plan](SIPHON_BINARY.md) starts with storage and runtime lifecycle. B2.2 makes the same
binary run host-defined programs and handle their requests. The Rust package handles types and serialization;
the native library runs phases, commits their changes and endings together, and owns interruption and recovery.
Octoii's current source dependency remains until that public interface can run the product.

## B2.2a: Program execution

Register programs on a `Builder` before opening the runtime, with one active version per kind. Reuse the existing
erased dispatch path for these programs and the native Rust programs. Keep stop handling and retries in the existing
runtime. `Changes::start` queues a process start, `Changes::terminate` queues its stop signal, and a `Snapshot`
reads the terminal outcome by process path. A process path is an address, not a promise of identity after reuse.
The native library constructs paths; callers do not construct paths from Beak's storage layout.

A phase is a synchronous callback with its input, optional checkpoint, and a borrowed `Run`. It reads the phase's
snapshot, adds changes to that phase, and sets one ending. The first slice covers `Next`, `Wait`, `Done`, `Fail`,
and `Stopped`, with waits for a child, a change, or a deadline. An error or caught panic throws the phase and
discards its staged changes; an explicit `Fail` remains an ending with the existing commit semantics. The
`Run` cannot commit separately. Request mail and stream operations enter with their corresponding interfaces.

The wrapper encodes input, checkpoints and results and borrows the `Run` only for the callback. C passes slices
for input and uses native setters that copy outgoing bytes before the callback returns. Registration transfers
callback context ownership once; the native library releases it once after the last invocation, including when
startup fails. Restored phases may run before open returns; their callback context must already be ready to use.
Safe Rust callbacks catch unwinding panics before returning to C. A C host must obey the callback and handle
lifetimes.

Phase callbacks use `Run` methods instead of blocking runtime calls. Guard synchronous open, commit, snapshot
and close before waiting on the worker when called from a callback or its context destructor. Explicit close
outside a callback waits for shutdown. If the last runtime owner is released inside a callback, drop the worker's
sender and let the existing worker tear down after the callback returns. There is no synchronous join in that
case. The worker keeps registrations alive until teardown completes.

Change the unpublished development ABI from 1 to 2, updating both headers, the native version, the artifact
manifest, and the wrapper's version check in the same slice. Use one C builder/open path for both an empty runtime
and one with programs. Consumer checks use the native archive as the sole implementation dependency.

## B2.2a private core scope — confirmed

Paths below are relative to the root of `devxiyang/octoii-siphon`:

| Files | Change |
| --- | --- |
| `crates/siphon-runtime/src/lib.rs`, `crates/siphon-runtime/src/program.rs`, `crates/siphon-runtime/src/runtime/builder.rs`, `crates/siphon-runtime/src/changes.rs` | Make the existing erased Program adapter available to the FFI crate, let registration own kind names, and share the same start primitive with the typed API. |
| New `crates/siphon-ffi/src/builder.rs`, `crates/siphon-ffi/src/program.rs` | Register and own host callbacks; translate scoped phase input, staged changes and endings. |
| `crates/siphon-ffi/src/lib.rs`, `crates/siphon-ffi/src/abi.rs`, `crates/siphon-ffi/src/runtime.rs`, `crates/siphon-ffi/src/changes.rs`, `crates/siphon-ffi/src/snapshot.rs`, `crates/siphon-ffi/include/siphon.h` | Expose builder startup, callback-safe shutdown, start and stop, and snapshot outcomes. Update the ABI and its ownership contract. |
| New `crates/siphon-ffi/tests/program.rs`, `crates/siphon-ffi/tests/c/program.c`; existing `crates/siphon-ffi/tests/lifecycle.rs`, `crates/siphon-ffi/tests/c/lifecycle.c` | Exercise callbacks from separately built hosts; migrate the storage tests to the same builder/open path. |
| `crates/siphon-ffi/Cargo.toml`, `Cargo.lock`, `README.md`, `.github/workflows/check.yml` | Add only dependencies needed by the adapter and tests; document and verify the updated ABI and native consumer in CI. |

An extension can only register Rust programs with the source-built runtime. The current native library opens a
runtime with no registrations, so a public consumer cannot execute a program through the existing binary. A small
registration adapter and ABI changes are required. Beak, the dispatcher, request handling and storage formats
are outside this slice.

In `octoii-works/octoii-siphon-lib`, update the matching header, raw declarations, `build.rs` and safe Rust wrapper;
add typed Program/Run adapters and tests. The product keeps its current runtime pending the remaining slices.

## B2.2a acceptance

- A separately linked C program and a Rust consumer built without private sources register a real program, start
  it, advance a checkpoint through `Next` and `Wait`, and see its result and phase writes in the same snapshot.
  The C consumer also closes and reopens storage while the program is waiting, then resumes from its checkpoint.
- Stopping a waiting program runs its stop handler and preserves its result. A callback that fails with an error
  after staging a write leaves that write unapplied. The Rust consumer also verifies caught panics.
- Callback reentry returns an error before waiting on the worker. Explicit close, release during a callback, and
  failed startup release registrations exactly once. A persistent runtime can reopen and read its committed
  outcome after a normal close and release of its snapshots.
- Existing native runtime and storage tests still pass. Headers and manifest agree on ABI 2. Clean local build
  outputs and test state as described in [CONTRIBUTING.md](../CONTRIBUTING.md#local-verification-cleanup).

## B2.2a verification

Private implementation: `74e4e8f`. Public wrapper: `be03497`. Private CI pins the exact wrapper commit.
On macOS ARM64 with Rust 1.100.0, the real ABI 2 archive passed both independently linked C consumers and
all 8 public Rust integration tests. The private runtime and FFI passed 20 tests, and both repositories
passed formatting and strict Clippy for the changed crates. The existing Octoii library compile and the
workspace dependency boundary check passed. The local Rust build directories, test binaries and state
were removed after verification. No binary or Cargo release was published.

## Following slices

- **B2.2b — Host Requests.** Reuse the current request mailbox and delivery. Give each delivered request a
  one-shot answer handle and let the host run its async handler. Stop signals cancellation and still allows an
  answer with a partial result. On reopen, an unanswered request becomes `Interrupted` through the current
  recovery path; the host does not receive it again. Verify that a late or duplicate response cannot answer
  another delivery and that shutdown does not wait on a host handler.
- **B2.2c — Streaming and person.** Expose the existing stream and person interfaces required by Octoii. Verify
  stream ordering and human answers through the binary. Machine and processing work follows in B3, then artifact
  validation on macOS, Windows and Linux and the product switch in B4.

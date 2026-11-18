# Siphon binary cutover

2026-11-18

Octoii will build and run against the published `octoii-siphon-lib` and `octoii-ai` packages. The private
Siphon repository owns Beak, the runtime, the native ABI implementation, and its build pipeline. The public
Siphon package owns the typed Rust interface and distribution of the corresponding binaries. Octoii owns
agent behavior, extensions, permissions policy, node composition, and the client protocol.

Each release uses one current contract at every boundary. Change the type, writer, reader, and callers together.
Require the current volume format, the exact native ABI and target, and the current client protocol at their
entry points. An unsupported version gives a clear error before runtime startup or state mutation. Keep fault recovery
for valid state in the current format. Validate a newly initialized volume and its subsequent reopen as part of
the cutover.

## Current evidence

- The product Git tree contains one private `siphon` submodule. Its `beak/` directory is ordinary source in
  the private repo. Cargo metadata resolves 20 product packages. `octoii` and `octoii-node` both declare
  direct path dependencies on Beak and Siphon Runtime. The product CI reads the gitlink, checks out the
  private repo with `SIPHON_DEPLOY_KEY`, and tests all three source workspaces.
- The local public wrapper at `42e61c2` and development ABI 4 cover storage writes, retained snapshots, fibers,
  host request callbacks, and reopen. The ABI still lacks the product's snapshot queries, journal, streaming,
  person replies, and machine operations. The wrapper's build reads a local archive and verifies the ABI,
  target, and hash. It rejects native linking outside macOS. Native binary consumers have run on macOS ARM64.
- The public default branch is `ee0b752` and the release list is empty. The crates.io lookup for
  `octoii-siphon-lib` returns 404. The product therefore has no published artifact to switch to yet.
  [GitHub releases](https://github.com/octoii-works/octoii-siphon-lib/releases) · [crates.io lookup](https://crates.io/api/v1/crates/octoii-siphon-lib)

## Remove the old contracts and close the source boundary

| Area | Observed behavior | Cutover action |
| --- | --- | --- |
| Beak request recovery | `beak-process/src/scheduler.rs` empties occupied answer slots on reopen to repair mail written by the old rules. The current request transaction opens an empty slot and reserves it for the exact reply. | Remove the old-state repair. Verify current-format wait recovery and retelling. Reject a broken slot invariant before dispatch. |
| Fiber persistence | Previous revisions used `system.program` and the `"program"` checkpoint spelling for a Fiber. | Completed in the Fiber label and volume format batch below. Beak uses `system.fiber`, Octoii reads the typed process record, and the new format is checked at open. |
| Person replies | Siphon still exposes an address-based human answer call. Octoii scans a thread by mailbox key, and clients submit that key. | Replace this route with the exact committed request identity throughout the prompt and reply. Use the same snapshot for the prompt, schema and identity, then verify the identity again in the reply transaction. |
| Streams | Siphon retains output, offsets and close handling by mailbox key. The client also deduplicates by that key and expects contiguous offsets. A subscription to one topic can see gaps in a request-wide offset sequence. | Carry the request identity and offsets through live delivery, replay, closure and reconnect. Use the mailbox key for business correlation. The client tracks the highest accepted offset for the exact request and accepts later offsets on the subscribed topic. |
| Octoii persisted settings and threads | A child missing its kind becomes General Purpose. A stored disabled-extension list silently filters out `base`. Both historical cases have explicit regression tests. | Require a child kind and validate the mandatory base extension on persisted reads and writes. Keep the current creation defaults for fields that are optional by contract. |
| Raw kernel access | Octoii opens Beak before starting Siphon, puts `Kernel` in the extension registrar, and uses `Runtime::kernel()` for commands, namespaces and IDs. Browser, Claude Code and Terminals take command handles during registration. Node accesses native snapshots and changes for files and sessions. | Register callbacks and assemble extension configuration through the public runtime builder. Give services typed volume, ID and machine capabilities before any resumed work can call them. Node and extensions use Octoii's product interfaces. Retire the raw kernel setup path after its callers move. |

The inspected Beak root, directory, change and process decoders accept a single format at each decoder.
Fiber version checks hold an incompatible process before running its code. Reopen of the confirmed log prefix,
run admissions, the first terminal error, and per-platform unsupported errors belong to the current contract.
Siphon and Beak's Cargo dependencies point toward the kernel, with product policy and AI integrations compiled
in Octoii and its extensions.

## Implementation order

1. **Finish B2.2c.2 and settle the current format.** Send the committed request identity with every streamed
   event and person prompt. Carry it through deduplication, close handling, and pending form state. Send the same
   identity back on reply and verify the request belongs to the selected thread. Keep business keys for tool
   and timeline correlation. Give clients a lossless wire identity and regenerate their protocol bindings with
   the new subprotocol. Define how a fresh subscription restores the active streams, including a close
   missed during reconnect. Exercise two requests sharing a key, a replacement while the browser still holds
   the old prompt, closed streams, and replay after a gap. Remove the confirmed legacy behaviors in small
   scoped batches before freezing the public contract and storage format.
2. **Fill the public SDK gaps from actual callers.** Extend the C declarations and safe Rust wrapper together.
   Group the work around capabilities the product already needs:

   | Capability | Product use |
   | --- | --- |
   | Volume and snapshot | Indexed queries, directory metadata, range reads, process and wait state, committed journal, volume IDs, and complete changes for threads, accounts, settings, files and access records. |
   | Runtime and requests | Fiber registration and phase-scoped changes, mail and wait operations, host callbacks, exact person replies, streams and cancellation, and deliberate shutdown and reopen. |
   | Machine | Namespaces and file views, path restrictions, command lifetimes, terminal I/O, and coprocess channels. Use the product's policy to configure these primitives. Verify ownership before launching or reclaiming commands across a host crash. |

   Bring up a clean product build using the resulting public wrapper and a locally built binary candidate.
   Register the real Octoii fibers and handlers. Exercise a model turn, a tool result, human approval, reconnect,
   stop, and reopen. This exposes missing API before the release while preserving the one production dependency
   switch.
3. **Build and verify distribution in parallel with the SDK work.** Private CI builds platform archives and
   hands verified artifacts to a clean consumer job. Start native link checks with the current development
   interface as the SDK expands. Required targets are macOS ARM64 and x64, Windows x64 MSVC, and Linux x64
   GNU. The current Windows implementation reports unsupported for restricted commands, terminals and the
   descriptor channel. Provide the required platform implementations and test them with the same ownership and
   permission contract. Declare minimum OS and libc versions from the native jobs.

   Release the wrapper and artifacts together. Pin the artifact version, ABI, target and checksum in the
   published wrapper's release metadata. Isolate private symbols and test a standalone C consumer and Rust
   consumer per target. An artifact setup step in the product build fetches and verifies the matching public
   archive and passes its directory to Cargo. The SDK verifies it against the pinned metadata before linking.
   Use this setup path for both the standalone consumer and the product.
4. **Cut Octoii over in one merge.** Replace the product Cargo path dependencies with the published wrapper,
   migrate the Node and Octoii callers, and regenerate the product lockfile. Remove the `siphon/` gitlink,
   the matching `.gitmodules` entry, the private checkout and deploy-key use, and the root commands that assume
   private workspaces exist. The private repo continues to build and test Siphon and Beak. Product CI builds
   and tests its real clients with public registry dependencies and release artifacts.

Each private implementation batch lists its exact files and technical dependency before approval under
`AGENTS.md`. Product code uses the new API as each public capability is adopted.

## Fiber label and volume format batch

Beak writes the Fiber kind to the process directory's `system.fiber` label and uses it for the scheduler's
process lookup. Octoii queries the process record on the Siphon runtime's snapshot when stopping a turn. Its
`State::Fiber` checkpoint uses the enum's current `"fiber"` spelling.

The label is part of the persisted volume contract. The frame decoder can still read a well-formed old
volume, while the new index query would omit the processes it contains. Check a required volume-format
marker under the existing directory lock before rebuilding indexes, trimming the log, or opening the
scheduler. A directory containing log or index data needs the current marker. Report an unsupported format
for unmarked or mismatched data. Initialize a fresh volume by publishing and syncing the marker before
creating its first segment. Use a temporary file and atomic rename as with the sealed segment index.
Sync the segment names before open succeeds, including on retry after an interrupted initialization. The
segment header continues to identify the record encoding.

Confirmed private scope, relative to `devxiyang/octoii-siphon`

| Files | Change |
| --- | --- |
| `beak/crates/beak-process/src/{commit.rs,scheduler.rs}`, `beak/crates/beak/src/layout.rs` | Write and query the new internal Fiber label. Remove the layout export now that the caller uses the typed query. |
| `beak/crates/beak-process/tests/scheduler.rs` | Check the literal persisted label and that the current scheduler discovers the process on reopen. |
| `beak/crates/beak-storage/src/{lib.rs,log.rs}`, `beak/crates/beak-storage/tests/log.rs` | Define the format error, validate or initialize the marker at open, and cover read-only rejection of unmarked and mismatched logs, interrupted initialization, and current-format reopen. |

Octoii changes `libs/octoii/src/threads/thread.rs` to use the snapshot's process record. Update the checkpoint spelling
and its regression in `libs/octoii/src/tools/calls.rs` and `libs/octoii/src/tests/calls.rs`. Verify the
existing stop-turn behavior and link the current binary consumers. The label and
volume open contract belong to Beak. An extension cannot change the persisted index key or stop an
incompatible volume from entering recovery.

Private commit `3fd4c6f`. The storage regressions verify rejection before recovery writes and cover failures
through each initialization step. A failed directory sync could leave a visible segment whose name would
be lost on power loss. The open completion now syncs the names before handing out the writer. The scheduler
regression uses `system.fiber` after reopen, and the Octoii checkpoint regression accepts `"fiber"`.
On macOS ARM64, Beak passed 164 tests, Siphon passed 30, and Octoii passed 105 library tests with one
live model test ignored. Three standalone C consumers and 13 public Rust consumer tests passed against
the ABI 4 archive. Formatting, scoped strict Clippy, Node's all-target check and the dependency check passed.

## Completion checks

- A clean product clone builds the Node and desktop packages with public credentials only. The git tree has
  no `siphon/` gitlink. The resolved product and test graph contains neither `beak`, `siphon-runtime` nor
  `siphon-ffi` from source. Check normal, build, dev, optional, and platform declarations.
- Product CI uses the same packaged artifacts tested by the independent public consumer. A mismatched
  checksum, target, ABI, volume format, or client protocol produces the expected error.
- Real product tests cover snapshots and journal, request approval with current identity, stale reply
  rejection, reconnect and stream replay, sandbox denial, terminal I/O, shutdown and reopen.
  Temporary build artifacts are cleaned after verification. Reusable Cargo caches remain in their owning
  workspaces. [Local verification cleanup](../CONTRIBUTING.md#local-verification-cleanup)

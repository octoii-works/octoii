# Siphon binary cutover

2026-11-18

Octoii will build and run against the published `octoii-siphon-lib` and `octoii-ai` packages. The private
Siphon repository owns Beak, the runtime, the native ABI implementation, and its build pipeline. The public
Siphon package owns the typed Rust interface and distribution of the corresponding binaries. Octoii owns
agent behavior, extensions, permissions policy, node composition, and the client protocol.

Development and releases use one current contract at every boundary. Change the type, writer, reader, and callers together.
Require the current volume format, the exact native ABI and target, and the current client protocol at their
entry points. An unsupported version gives a clear error before runtime startup or state mutation. Keep fault recovery
for valid state in the current format. Validate a newly initialized volume and its subsequent reopen as part of
the cutover.

## Product boundary

Octoii builds through the public `octoii-siphon-lib` SDK. Its Cargo graph uses the typed
interface for snapshots, transactions, fibers, host requests, person replies, streams and
machine operations. The dependency check rejects Beak and Siphon implementation packages
in the resolved graph. CI installs the public archive for the locked SDK version.

The private Siphon repository owns its Beak workspace and builds the native archives. The
SDK crate carries the target and checksums of each matching distribution. The product has
no private checkout dependency or deploy key.

The published SDK is version `0.0.1`. Its four native archives passed the private native
pipeline's C and Rust consumer checks. [Cargo package](https://crates.io/crates/octoii-siphon-lib/0.0.1) ·
[Binary release](https://github.com/octoii-works/octoii-siphon-lib/releases/tag/v0.0.1)


## Remove the old contracts and close the source boundary

| Area | Observed behavior | Cutover action |
| --- | --- | --- |
| Beak request recovery | The scheduler requires an empty answer slot for each pending request before it resumes fibers. | Valid requests are retold by their committed identities. A conflict rejects reopen before the scheduler writes a recovery commit. |
| Fiber persistence | Previous revisions used `system.program` and the `"program"` checkpoint spelling for a Fiber. | Completed in the Fiber label and volume format batch below. Beak uses `system.fiber`, Octoii reads the typed process record, and the new format is checked at open. |
| Person replies | B2.2c.2 replaced address-based replies with the exact committed identity. Siphon reads the prompt and schema from one snapshot, then validates that identity again in the reply transaction. | Expose the same typed query and answer through the public SDK. |
| Streams | B2.2c.2 bound live output, replay and close to the exact request. The client accepts increasing offsets with gaps and resets live state on resubscribe. | Expose the stream contract through the public SDK. |
| Octoii settings and sub-threads | Settings readers and writers reject a disabled list containing `base`. Sub-thread creation and metadata require a kind. | The tool schema, metadata reads, and settings reads and writes enforce these requirements. |
| Thread startup claims | The claim fingerprint includes the variant of every input part. | Replays must match the current fingerprint for text and attachments. |
| Raw kernel access | Octoii opens Beak before starting Siphon, puts `Kernel` in the extension registrar, and uses `Runtime::kernel()` for commands, namespaces and IDs. Browser, Claude Code and Terminals take command handles during registration. | Register callbacks and assemble extension configuration through the public runtime builder. Give services typed volume, ID and machine capabilities before any resumed work can call them. Retire the raw kernel setup path after its callers move. |

The inspected Beak root, directory, change and process decoders accept a single format at each decoder.
Fiber version checks hold an incompatible process before running its code. Reopen of the confirmed log prefix,
run admissions, the first terminal error, and per-platform unsupported errors belong to the current contract.
Siphon and Beak's Cargo dependencies point toward the kernel, with product policy and AI integrations compiled
in Octoii and its extensions.

## Implementation order

1. **Settle the current format.**
   [B2.2c.2](SIPHON_LIFECYCLE.md#b22c2-bind-streams-and-browser-replies-to-request-identity) migrated Siphon,
   Node and client source consumers to the exact request identity. The product settings and metadata batch
   and the request slot recovery batch enforce the current contracts described below.
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
   GNU. Windows validates the portable volume and Fiber contract. Restricted commands, terminals and the
   descriptor channel use the macOS and Linux backends.

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

## Cutover acceptance

- The product worktree passed 257 Rust tests with two optional external tests ignored.
- The client passed nine tests against a real Node. The macOS desktop package built and its
  embedded Node ran.
- The manifest installer and Cargo build verify the published target archive. The Rust
  dependency check examines normal, development, build and target-specific dependencies.
- Restricted commands, terminals and descriptor channels use the platform backends that
  implement them. Windows currently exposes the portable storage and execution interfaces
  and reports unsupported for these machine features.

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

## Product settings, sub-thread metadata, and startup claims

Both the settings reader and writer validate the disabled-extension list. A list containing `base` is invalid.
A stored sub-thread must declare its scope explicitly. Its parent metadata requires the kind, and a missing or
unknown kind stops the read before tool selection. The `delegate` tool requires a kind for its `start` action.
Startup claims hash tagged text and attachment parts. A replay must match the current request fingerprint.

The Octoii workspace passed 256 tests on macOS ARM64. The external model and Claude Code tests were ignored.
The real Node acceptance script verified replies, restart with a pending approval, and stream replay after
disconnect.

## Request slot recovery batch

Every pending request reserves an empty slot for its answer. On scheduler open, validate that slot against the
recovered snapshot. If the slot is occupied, the scheduler refuses to open and reports the path before restarting
admitted fibers or delivering notices. A valid pending request is retold with its committed identity. Creating a
new request under a reused key consumes the previous reply as part of the request transaction.

Confirmed private scope, relative to `devxiyang/octoii-siphon`

| Files | Change |
| --- | --- |
| `beak/crates/beak-process/src/scheduler.rs` | Validate pending answer slots before recovery commits and retell valid requests by identity. |
| `beak/crates/beak-process/src/lib.rs` | Add an occupied-slot error with the slot path. The existing volume and runtime error path carries it to the caller. |
| `beak/crates/beak-process/src/commit.rs` | Keep answer-slot clearing private to the transaction that creates a request. |
| `beak/crates/beak-process/tests/scheduler.rs` | Replace the legacy repair case with refusal before a recovery commit, and verify retelling on a valid reopen. |

The scheduler owns recovered state before the volume publishes notices. Extensions enter after this boundary.

Private commit `3486103`. A current-format reopen retells a pending request without adding a recovery commit
when no fiber was admitted. With an admitted fiber and an occupied slot, open returns the slot path. Reading the
same underlying device confirms that both the commit sequence and the conflicting data are intact.
On macOS ARM64, Beak passed 165 tests and Siphon passed 33. The three C consumers and 13 public Rust tests
passed against the ABI 4 archive.

## Snapshot directory reads

The Node attachment routes load and keep thread files through Octoii's `Volume`. Image fitting and
content-addressed storage use the same primitives as attachments accepted with a thread's first input.

Octoii reads volume entries to find threads, load their configuration and settings, page their inboxes,
and serve attached files. `stat` reads one entry and `list` reads a directory page. Both retained snapshots
and the snapshot borrowed during a Fiber phase use the same read interface.

The C boundary sends an entry through a synchronous visitor. The entry name, length, expiration time when
set, and directory metadata when present are valid for the duration of the callback. Labels and annotations use byte slices.
The Rust SDK copies results into owned entries. List options carry the prefix, optional exclusive cursor,
direction, and limit. Missing paths return the existing missing status. A present empty directory returns
success with zero visits. The transfer exposes no Beak handles or storage encoding.

Confirmed scope

| Repository | Files | Change |
| --- | --- | --- |
| Private Siphon | `crates/siphon-ffi/src/snapshot.rs`, `crates/siphon-ffi/include/siphon.h`, `crates/siphon-ffi/tests/c/lifecycle.c` | Expose the two snapshot calls and test the borrowed entry data and pagination against the binary. |
| Public SDK | `src/{lib,sys,fiber,snapshot}.rs`, `include/siphon.h`, `tests/{storage,fiber}.rs` | Bind the C calls, keep snapshot reads together in `snapshot.rs`, and test owned results from both retained and phase snapshots. |

The private FFI already holds the immutable snapshot. Only it can implement these reads through a linked binary.
An extension cannot supply the missing symbols. Verify an empty result and a missing path, raw byte names,
directory labels and annotations, forward and reverse pagination, and results retained after the runtime closes.
Index queries, journal cursors, and additional write primitives are separate batches.

Private Siphon commit `e38cc68` and public SDK commit `af8212e`. On macOS ARM64, all three independent C
programs and 14 Rust SDK tests passed against the native ABI 4 archive. The public header matches the
private header. The Rust tests exercise both retained snapshots and a borrowed Fiber snapshot.

The private workflow compares both headers and builds a public consumer pinned to `42e61c2`. It needs the
verified SDK revision when preparing the matching revisions for publication.

### Reverse byte-prefix paging

The independent Rust consumer found that Beak's reverse range skips names such as `p\xff` when paging
under the prefix `p`. The tree seeks below the prefix with a trailing `0xff`, which is itself a valid
matching name. Compute the exclusive upper bound by incrementing the last byte below `0xff` and omitting
the trailing bytes. An empty or entirely `0xff` prefix has no finite upper bound. The cursor can narrow
the resulting range.

Confirmed private correction `3400fbd` in `beak/crates/beak-tree/src/btree.rs` and
`beak/crates/beak-tree/tests/tree.rs`. Beak and the independent Rust consumer verify reverse pages across
byte prefixes, exclusive cursors, and the end of the key space. The ABI uses the same bounded tree read as
Rust callers. Beak passed 166 tests, Siphon passed 34, and the Octoii workspace passed 256 with the two
external integration tests ignored.

## Indexed labels and metadata changes

Octoii reads the directory, status and pending prompts of a thread from one immutable snapshot. Startup
state and attachment access use the same directory lookup, which excludes threads being removed.

Thread deletion finds descendants by parent label. Settings and thread creation also write directory
metadata. The binary snapshot exposes `find`, and the atomic changes batch exposes `meta`. Native Rust,
C and the public Rust SDK use the same label index and metadata transaction. Both operations are available
during a Fiber phase.

`find` takes the directory kind, label name, optional exact value, subtree, exclusive path cursor and limit.
Results are ordered by full byte path. Each result includes the matched label value and the same entry view
used by `stat` and `list`. `meta` accepts label and annotation pairs. In the C call, a null zero-length value
means removal and a present zero-length value means an empty value. In the Rust SDK this distinction is
`Option<&[u8]>`. A phase reads from its immutable snapshot while its writes are staged for commit.

Confirmed scope

| Repository | Files | Change |
| --- | --- | --- |
| Private Siphon | `crates/siphon-ffi/src/{abi,changes,snapshot}.rs`, `crates/siphon-ffi/include/siphon.h`, `crates/siphon-ffi/tests/c/lifecycle.c` | Bind an indexed query and one metadata operation, reusing borrowed entry views and the native change batch. |
| Public SDK | `src/{lib,sys,fiber,snapshot}.rs`, `include/siphon.h`, `tests/{storage,fiber}.rs` | Expose typed queries and named metadata changes to retained snapshots, batches and Fiber callbacks. |

Extensions cannot provide new binary symbols or publish metadata and index updates in the same native
transaction.

Private commit `f71f1cb` and public commit `44bf843`. The independent tests exercise full-path paging,
subtree boundaries, searches for any value, searches for empty or exact byte values, and metadata removal.
Retained snapshots keep their own index view. A rejected batch preserves the previous metadata and index.
Fiber tests read the commit that follows a staged change. On macOS ARM64, 34 Siphon tests, three C consumers,
and 15 standalone Rust SDK tests passed against the current build. Strict Clippy passed for both workspaces.

## Committed journal pages

Thread, thread-list and settings subscriptions read the volume journal to decide which views to refresh.
The binary snapshot exposes a page of committed change sets after an exclusive sequence cursor. Each set
carries its sequence and the byte paths it changed. The page reports the sequence read through, the oldest
retained sequence when present, the snapshot sequence, and whether the reader missed history. Progress is
independent of the number of paths, so an empty change set can still advance the cursor.

The C visitor borrows one bounded page for the duration of a synchronous call. The Rust SDK copies the
result into an owned journal page. Octoii advances its cursor after reading through the snapshot. A history gap
or a page that cannot make progress triggers a refresh. The thread-list baseline and its updates use the
same snapshot as their journal cursor.

Confirmed scope

| Repository | Files | Change |
| --- | --- | --- |
| Private Siphon | `crates/siphon-ffi/src/snapshot.rs`, `crates/siphon-ffi/include/siphon.h`, `crates/siphon-ffi/tests/c/lifecycle.c` | Bind the existing unfiltered snapshot journal and verify its page boundary through C and a native retention test. |
| Public SDK | `src/{lib,sys,snapshot}.rs`, `include/siphon.h`, `tests/storage.rs` | Expose the owned page and verify bounded reads, byte paths, empty pages, and monotonic exclusive cursors. |

The binary owns access to the snapshot journal and its retention information.

Private commit `1631489` and public SDK commit `9be72b8`. The native test covers a pruned history gap and the
cursor advancement of an empty commit. The independent consumers verify paging, grouping paths by commits,
preserving byte paths, and reading through the sequence of a retained snapshot. On macOS ARM64, 35 Siphon
tests, three independent C programs and 16 standalone Rust SDK tests passed. Strict Clippy passed for Siphon
and the SDK. The Octoii workspace passed 256 tests and strict Clippy. Its two external integration tests
were ignored.

## Fiber records

Octoii reads Fiber records to identify a turn, fence a delivery, show usage and a scheduled resume, and
decide when an ended Fiber can be reaped. The snapshot and the borrowed view in a phase expose the same
record. It carries the kind, version, creation sequence, optional wait deadline, usage counters, outcome,
and whether reaping is available. A Fiber can have an outcome while its descendants are still exiting.

The C visitor lends the record for the duration of the call. Missing Fibers use the existing missing
status. Optional byte fields distinguish an absent result or reason from present empty bytes. The Rust
SDK owns the returned record and uses it to decode typed outcomes for the registered kind and version.
The record replaces the outcome-only C symbol.

Octoii uses reaping eligibility to settle turns and tool Fibers and to recycle a compaction or resume slot.
An exiting Fiber keeps its slot until its descendants have ended.

Confirmed scope

| Repository | Files | Change |
| --- | --- | --- |
| Private Siphon | `crates/siphon-ffi/src/snapshot.rs`, `crates/siphon-ffi/include/siphon.h`, `crates/siphon-ffi/tests/c/{fiber,request}.c` | Bind the existing snapshot record and migrate C consumers from the outcome-only function. |
| Public SDK | `src/{lib,sys,snapshot,fiber}.rs`, `include/siphon.h`, `tests/fiber.rs` | Expose records and decode typed outcomes from the same native read. |

The binary owns the recorded Fiber lifetime and usage.

Private commit `10981dd` and public SDK commit `0528bc3`. The native test checks committed usage and the
presence of an empty result. The standalone SDK checks a wait deadline, kind validation, and a recorded
outcome before and after reaping becomes available. The archived binary exports `siphon_snapshot_fiber`
as its single Fiber record reader. On macOS ARM64, 36 Siphon tests, three C consumers, 17 standalone Rust
SDK tests, and 257 Octoii tests passed. The two external Octoii tests were ignored. Strict Clippy passed
for all affected packages.

## Volume IDs and retention

Octoii allocates IDs from the volume both between turns and inside a Fiber phase. Node uses the runtime clock
when expiring pairing links and sessions. Thread logs seal files and schedule old segments to expire. Expose
these operations through the existing runtime, phase and changes handles. Expiration takes an optional instant
in milliseconds. Setting an expiration or clearing it is part of the existing atomic batch.

Confirmed scope

| Repository | Files | Change |
| --- | --- | --- |
| Private Siphon | `crates/siphon-ffi/src/{changes,runtime,fiber}.rs`, `crates/siphon-ffi/include/siphon.h`, `crates/siphon-ffi/tests/c/{lifecycle,fiber}.c` | Bind runtime and phase ID allocation, runtime time, sealing, and expiration. |
| Public SDK | `src/{lib,sys,fiber}.rs`, `include/siphon.h`, `tests/{storage,fiber}.rs` | Wrap the existing handles and verify IDs, sealed writes, and expiration changes against the archive. |

The binary owns the committed ID sequence, time source and file mutation transaction. Acceptance covers
runtime and phase IDs from one volume, reopening, close and callback safety, sealed file refusal, and setting
and clearing expiration on retained snapshots.

Private commit `fea8ec4` and public SDK commit `7c65fac`. On macOS ARM64, 36 Siphon tests, three
independent C programs and 19 standalone Rust SDK tests passed. The product startup change passed 257 tests
with two external tests ignored. Strict Clippy passed for all three workspaces.

## Fiber lifecycle changes

The product modules name their ordinary root directories for sessions, threads, accounts, project grants,
and recorded command groups.

Octoii sends a signal with a reason when stopping a turn, requests removal of a running subtree,
forces removal when deleting a thread, reaps ended Fibers, and charges usage counters. Stage each
operation on the existing atomic changes batch or on the batch borrowed during a phase.
Graceful removal gives stop handlers a chance to run. Forced removal kills the live Fibers under the
path. Reap applies to a terminal, reapable Fiber record. Counter names are byte slices and their amounts
are unsigned 64-bit values.

The C signal call takes an explicit signal and an optional byte reason. It replaces the termination-only
symbol. The Rust SDK exposes the same operations through `Changes` and `Run`. The kernel decides when
signals, subtree removal, reaping and charging take effect within the transaction.

Confirmed scope

| Repository | Files | Change |
| --- | --- | --- |
| Private Siphon | `crates/siphon-ffi/src/changes.rs`, `crates/siphon-ffi/include/siphon.h`, `crates/siphon-ffi/tests/c/{fiber,request}.c` | Bind signal, remove, force remove, reap, and charge. Migrate the native consumers to the signal call. |
| Public SDK | `src/{lib,sys,fiber}.rs`, `include/siphon.h`, `tests/{fiber,request}.rs` | Expose the batch and phase operations, and verify signaling, removal, reaping and usage against the archive. |

Private commit `6f79d2d` and public SDK commit `b025e59`. The binary consumers verify a stop reason,
refusal of an early reap, a reaped record's disappearance, and the behavior of both removal operations.
The SDK tests check batch and phase charges in committed Fiber records. On macOS ARM64, 36 Siphon tests,
three C consumers, 20 standalone Rust SDK tests and 257 Octoii tests passed. The two external Octoii
tests were ignored. Strict Clippy passed for all three workspaces.

## Ordinary Fiber mail

Thread completion reports read ordinary mail through `Run::mail` and check the message name before taking it.
A change batch can send a named payload to a Fiber, with
an optional creation sequence that fences the delivery to that incarnation. The same batch governs mail
sent from a phase. The phase reads the message from its immutable snapshot and stages consumption in its
changes. Mail keys, names, and payloads are byte strings. The keyed mail wait wakes when that entry arrives.

Request answers and ordinary mail use one native read of the mailbox entry. The request answer operation
decodes that entry according to its type, and the commit still owns the reservation for an unanswered request.
The product uses keyed mail, child, directory-change, and deadline waits. These cover its current wait sites.

Confirmed scope

| Repository | Files | Change |
| --- | --- | --- |
| Private Siphon | `crates/siphon-runtime/src/fiber.rs`, `crates/siphon-ffi/src/{changes,fiber}.rs`, `crates/siphon-ffi/include/siphon.h`, `crates/siphon-ffi/tests/c/fiber.c` | Share the native mailbox read, expose delivery and phase reads, and accept byte keys for staged consumption. |
| Public SDK | `src/{lib,sys,fiber}.rs`, `include/siphon.h`, `tests/{fiber,request}.rs` | Expose owned mail, send and consume it through the phase and change batch, and exercise byte-keyed waits. |

The commit owns the incarnation check and the reservation for a pending answer.

Private commit `e33d812` and public SDK commit `227fe4f`. Independent consumers verify a byte-keyed message
waking a phase, consumption becoming visible after commit, refusal of a wrong creation sequence, and protection
of a reserved answer slot. The phase test also sends mail through its own batch. On macOS ARM64, 36 Siphon
tests, three C consumers, 21 standalone Rust SDK tests and 257 Octoii tests passed. The two external Octoii
tests were ignored. Strict Clippy passed for all three workspaces.

## Commit notifications

Octoii needs a commit watch to wake its subscriptions and pending startup operations. Register the watch
on the binary runtime and return the initial sequence and subsequent latest committed sequences. The
snapshot journal gives the actual changes and detects a history gap. A notification can coalesce commits.

The C callback takes a sequence value and a context owned by the registration. Dropping the watch cancels
it. Closing the runtime ends the watch and releases its context. The Rust SDK returns an owned watch with
an asynchronous `changed` operation. Its caller can read the current sequence and follow the journal.

Confirmed scope

| Repository | Files | Change |
| --- | --- | --- |
| Private Siphon | `crates/siphon-ffi/src/runtime.rs`, `crates/siphon-ffi/include/siphon.h`, `crates/siphon-ffi/tests/c/lifecycle.c` | Expose the native committed watch with context release and cancellation. |
| Public SDK | `Cargo.toml`, `src/{lib,sys}.rs`, `include/siphon.h`, `tests/storage.rs` | Bind the watch, enable Tokio sync for the owned receiver, and verify commit, cancel and close. |

The binary runtime owns the durable commit signal.

Private commit `67d3648` and public SDK commit `c101440`. The C consumer checks release after an invalid
registration, callback reentry refusal, and independent cancellation. The Rust SDK verifies the initial
sequence, progress after successful commits, no notification after a rejected transaction, journal
visibility, and receiver closure at shutdown. On macOS ARM64, 36 Siphon tests, three independent C
consumers, and 22 standalone Rust SDK tests passed. Strict Clippy passed for Siphon and the SDK.

## Host request identity

Pass the identity already stored by the runtime when delivering a host request. It consists of the
Fiber path and creation sequence, and the request's byte key and creation sequence. The host can
retain this identity and compare it with a later observation of the same request. The reply handle
continues to belong to the specific delivery.

Confirmed scope

| Repository | Files | Change |
| --- | --- | --- |
| Private Siphon | `crates/siphon-ffi/src/request.rs`, `crates/siphon-ffi/include/siphon.h`, `crates/siphon-ffi/tests/{request.rs,c/request.c}` | Pass the exact identity to the host callback and verify both sequences in a C consumer. |
| Public SDK | `src/{lib,sys,request}.rs`, `include/siphon.h`, `tests/request.rs` | Replace the separate delivery path and key with an owned `RequestId`. Verify request identity alongside the existing answer and cancellation behavior. |

Both sequence values come from the transaction that recorded the Fiber and its request. An extension
cannot reconstruct them from the path and key alone, which can both be reused.

Private commit `bb6b171` and public SDK commit `d05e1f0`. The binary consumer verifies that two
requests in one Fiber share the same Fiber creation sequence and have distinct request creation
sequences. The Rust SDK checks that the creation sequence matches the snapshot's Fiber record.
On macOS ARM64, strict Clippy, 36 Siphon tests, three C consumers, and 22 standalone SDK tests passed.
The Octoii workspace passed strict Clippy and 257 tests, with two external tests ignored.

## Person requests

Register the schema for a request name before opening the runtime. The application supplies the schema
from the typed payload. Siphon lists prompts from the caller's retained snapshot, validates an answer
on the current request, and commits it against the same exact identity. Prompts remain visible in their
original snapshot after the current request is answered. An invalid answer leaves the request pending.

The native builder reserves each request name once across host and person callbacks. The schema
callback transfers its result bytes with a release function. Siphon decodes those bytes before calling
release. The retained Beak snapshot can be cloned to share the same immutable view with the ABI worker.

Private commit `588fa19` and public SDK commit `fc0f4fa`. C and Rust consumers verified schema-based
validation, stale identity and duplicate answer rejection. The SDK also verified the prompt's identity
after close and reopen, and the original snapshot after a valid answer. Strict Clippy passed for Beak,
Siphon, the SDK, and Octoii on macOS ARM64. Beak passed 166 tests, Siphon passed 36, all three C
consumers passed, the SDK passed 23, and Octoii passed 257 with two external tests ignored.

## Request streams

Only the host delivery can publish for its exact request identity. The publisher retains the stream
sender and identity, so a retained reply handle can still be released after runtime shutdown without
holding its volume open. After a reply is submitted or its delivery closes, publication is rejected.

The native follower reads on demand. It uses the existing replay, lag reset, live data and per-request
closure rules. The SDK keeps one pending read, including when the caller cancels an await. Dropping
the follower releases its pending callback and runtime shutdown ends its reads.

Private commit `93c1c8a` and public SDK commit `fa96d9e`. C and Rust consumers exercised retained
and live output, exact request closure, rejection of late publication, and cancellation. On macOS
ARM64, strict Clippy, 36 Siphon tests, three C consumers, and 24 standalone SDK tests passed.

## Machine boundary for the product switch

Octoii's file and command tools already operate through a configured namespace. The product decides
which host folders to mount, which paths to hide, and which read-only views of the volume to show.
The public interface exposes those namespace capabilities and executes commands within the same
namespace. Siphon owns the namespace and command handles. Terminal and coprocess operations keep
the existing command lifecycle, cancellation and shutdown contract.

Model, review, prompt gathering, and tool handlers also use the active runtime while they answer a
request. Runtime access must be available with the delivery even when a Fiber runs before `open`
returns to the application. Retaining an old delivery must leave volume close and reopen possible.

Private commit `20d804f` uses the same seek and bounded read on each freshly opened host file on all
platforms. A ranged write uses the file's cursor and writes the full input. This removes the Unix-only
file methods from the namespace without changing how it opens or confines host paths.

Strict Clippy for the Siphon workspace passed when targeting Linux ARM64 and Windows x64 MSVC.
The link and execution pipeline currently runs on macOS, and local native verification used ARM64.
Linux needs the packaged library and execution tests. Windows also needs command confinement and
terminal support, which Beak currently rejects on that platform.

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

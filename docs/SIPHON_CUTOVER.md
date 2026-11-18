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

## Current evidence

- The product Git tree contains one private `siphon` submodule. Its `beak/` directory is ordinary source in
  the private repo. Cargo metadata resolves 20 product packages. `octoii` and `octoii-node` both declare
  direct path dependencies on Beak and Siphon Runtime. The product CI reads the gitlink, checks out the
  private repo with `SIPHON_DEPLOY_KEY`, and tests all three source workspaces.
- The local public wrapper at `af8212e` and development ABI 4 cover storage writes, retained snapshots, fibers,
  host request callbacks, directory reads, and reopen. The product still needs indexed snapshot queries,
  the journal, streaming, person replies, and machine operations. The wrapper's build reads a local archive
  and verifies the ABI, target, and hash. It rejects native linking outside macOS. Native binary consumers
  have run on macOS ARM64.
- The public default branch is `ee0b752` and the release list is empty. The crates.io lookup for
  `octoii-siphon-lib` returns 404. The product therefore has no published artifact to switch to yet.
  [GitHub releases](https://github.com/octoii-works/octoii-siphon-lib/releases) · [crates.io lookup](https://crates.io/api/v1/crates/octoii-siphon-lib)

## Remove the old contracts and close the source boundary

| Area | Observed behavior | Cutover action |
| --- | --- | --- |
| Beak request recovery | The scheduler requires an empty answer slot for each pending request before it resumes fibers. | Valid requests are retold by their committed identities. A conflict rejects reopen before the scheduler writes a recovery commit. |
| Fiber persistence | Previous revisions used `system.program` and the `"program"` checkpoint spelling for a Fiber. | Completed in the Fiber label and volume format batch below. Beak uses `system.fiber`, Octoii reads the typed process record, and the new format is checked at open. |
| Person replies | B2.2c.2 replaced address-based replies with the exact committed identity. Siphon reads the prompt and schema from one snapshot, then validates that identity again in the reply transaction. | Expose the same typed query and answer through the public SDK. |
| Streams | B2.2c.2 bound live output, replay and close to the exact request. The client accepts increasing offsets with gaps and resets live state on resubscribe. | Expose the stream contract through the public SDK. |
| Octoii settings and sub-threads | Settings readers and writers reject a disabled list containing `base`. Sub-thread creation and metadata require a kind. | The tool schema, metadata reads, and settings reads and writes enforce these requirements. |
| Thread startup claims | The claim fingerprint includes the variant of every input part. | Replays must match the current fingerprint for text and attachments. |
| Raw kernel access | Octoii opens Beak before starting Siphon, puts `Kernel` in the extension registrar, and uses `Runtime::kernel()` for commands, namespaces and IDs. Browser, Claude Code and Terminals take command handles during registration. Node still uses native snapshots and changes for sessions. | Register callbacks and assemble extension configuration through the public runtime builder. Give services typed volume, ID and machine capabilities before any resumed work can call them. Node and extensions use Octoii's product interfaces. Retire the raw kernel setup path after its callers move. |

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

# Siphon lifecycle repairs

Baseline: private Siphon `795bdb1`, development ABI 3. The [request bridge](SIPHON_REQUESTS.md) keeps the
existing transaction and process lifecycle. Fix the defects below in Beak or Siphon before adding streaming
and human replies. Keep commits small and verify each independently. The current Beak scheduler and volume
suites passed all 23 tests on macOS ARM64 before either repair.

## R01: deliver only requests that survive their transaction

A request and a terminal process ending can be staged in the same phase. The Beak transaction removes the
request when the process ends, but its staged `Notice::Request` can still reach an external handler. A standalone
ABI 3 consumer reproduced one handler invocation in this case.

Before committing the tree, validate staged request notices against the final transaction view. Keep a notice
only if its request still exists under the same live process incarnation. This also covers a request answered
or withdrawn in the same batch. Preserve notice order for requests that survive, including cooperative stop
notifications. A read failure must follow the existing transaction failure path before data or notices become
visible. Reopen and its retold-request semantics are unchanged. Do not substitute a later runtime snapshot check
for the transaction's own final view. This batch does not redefine the existing same-key replacement behavior.

Exact private scope (relative to `devxiyang/octoii-siphon`):

- `beak/crates/beak-process/src/commit.rs`: reuse the kernel's process/request lookup to validate staged notices.
- `beak/crates/beak-process/src/scheduler.rs`: finalize the notice batch within the existing transaction.
- `beak/crates/beak-process/tests/scheduler.rs`: prove request + completion and request + answer do not emit a
  request notice; retain the positive checks for pending and retold requests.
- `crates/siphon-ffi/tests/c/request.c`: verify a phase that stages a request then completes never invokes the
  host handler, using the linked ABI 3 archive alongside the existing positive request case.

This repair does not close an already delivered request on later process exit. That needs a terminal delivery
event distinct from the existing cooperative stop notification.

## R02: make an invalid wait a recorded phase failure

Beak reserves a process as Running. If the phase's ending waits on a child it does not own, Beak returns
`WaitNotOwned`. The runtime reports the error and drops the result, while the scheduler still regards the
process as Running. The end of a failed phase should follow the existing throw/retry policy.

Classify `WaitNotOwned` at Beak's finish boundary, where the refusal is still typed. Roll back the original
phase batch, then record a throw with no writes using the existing path. Leave stale or no-longer-admitted
results refused. A throw may lead to another phase under the configured program policy; it must never commit
the rejected phase's changes. This is independent of retrying a temporary storage error.

Exact private scope:

- `beak/crates/beak-volume/src/volume.rs`: route the invalid ending to the existing thrown-phase commit.
- `beak/crates/beak-volume/tests/volume.rs`: reserve a real run, submit a non-owned child wait with a staged
  write outside the process subtree, and verify the recorded failure and rolled-back write with no retries
  configured. Process cleanup alone must not make the write disappear. Keep the existing stale or not-admitted
  result behavior.

R01 and R02 required core edits because extensions cannot change which notices Beak publishes or resolve runs
after Beak has reserved them. Both batches preserve the existing ABI, disk format, scheduler structure, and
product dependencies.

## R01 and R02 verification

Private commits: R01 `fcd5910`, R02 `b92a006`. The new R01 scheduler test failed against the old producer
and passed with the repair. All 19 Beak process tests, 10 Beak volume tests, and three separately linked ABI 3
C consumers passed. Formatting, scoped Clippy, and Octoii's dependency check also passed.

During verification, the runtime suite exposed a separate topic replay bug. A standalone probe reproduced it
on the unmodified `streams.rs` from baseline `795bdb1`. R03 resolves it before the product pin advances.

## R03: preserve topic and offset in retained stream events

Live output carries its own topic and request offset. The retained buffer stores only bytes, assigning the
request's first topic to every retained event on replay. Keep each complete published event in the retained
buffer and filter by its actual topic; preserve request-wide offsets, live delivery and the existing retention
limit. This fixes the observed defect without changing the stream ABI or taking on the buffering redesign.

Exact private scope, confirmed:

- `crates/siphon-runtime/src/streams.rs`: retain and replay the same event identity used by live subscribers.
- `crates/siphon-runtime/tests/runtime.rs`: make the existing stream test attach after a mixed-topic batch
  was queued, and verify that each topic sees only its own events at the original offsets.

Extensions cannot correct replay after the runtime has discarded the publishing topic. Verify the scoped
runtime and binary regressions before advancing the product pin.

Private commit `7f0942e`. The deterministic regression fails before the repair and passes after it. Both
topics receive only their own events at the original request-wide offsets, during replay and live delivery.
All 22 runtime/FFI tests, the three standalone ABI 3 C consumers, formatting and scoped Clippy passed locally.
Octoii also passed `cargo check --locked --offline -p octoii --lib` and `python3 tools/check-rust-layers.py`
against this revision.
[Private CI](https://github.com/devxiyang/octoii-siphon/actions/runs/44202024627) passed both workspace suites
and linked the ABI 3 archive into independent C and Rust consumers on macOS ARM64.

## R04: close a delivery when its request is removed

An independent C consumer linked against `7f0942e` received a request, then woke its process by creating a
directory that the process was watching. The next phase completed normally. A second later, the host still
had no `Closed` notification or watch release, and a late reply returned success (queued). The runtime had
never been closed. Beak had already removed the request in the commit that recorded completion.

Use one existing identity: process path, process creation sequence, and request key. A cooperative stop tells
the handler to return what it has, and its reply remains usable. Committed removal closes the delivery. A
reply racing removal can enter the receiver before it closes. Beak's existing answer guard still decides
whether that reply commits. The public ABI already has `Stop` and `Closed`; the latter closes the receiver
before notifying the host and releasing its watch.

At transaction finalization, use the tree's touched paths to find previously pending requests that may have
gone. Resolve those identities against the final transaction view, reusing the pending check from R01. This
covers answer, exit, kill, expiry and withdrawal through tree changes. A deleted directory can hold requests
at multiple depths. A recorded deletion by itself is insufficient because a transaction can restore the
same entry before it commits. Restrict the old-snapshot lookup to affected paths and subtrees. Keep the
lookup fallible before the transaction commits, and publish each closure once with the existing notices
after commit. Deliver closure before any new request for a replacement process at the same path.

In Siphon, close the matching handler task and release its retained output after the task can no longer
publish. Track completion against the specific task so an older completion cannot remove a replacement from
the live table. Stop retains its distinct cooperative token. Keep the binary-generated mailbox keys and
existing answer admission. Reusing a caller-chosen key within one live process remains outside this repair.

Confirmed private scope, relative to `devxiyang/octoii-siphon`:

| Files | Change |
| --- | --- |
| `beak/crates/beak-tree/src/tree/transaction.rs` | Expose existing touched paths to the scheduler. |
| `beak/crates/beak-process/src/{commit,scheduler}.rs`, `beak/crates/beak-process/tests/scheduler.rs` | Finalize request and closure notices together. Carry the process creation sequence on stop and closure. Verify closure after answer, removal and exit, deduplication, and no closure for rolled-back or restored entries. |
| `beak/crates/beak-c/src/queue.rs`, `beak/crates/beak-c/include/beak.h`, `beak/crates/beak/src/{queue,kernel}.rs` | Carry the notices over the private Beak event bridge. |
| `crates/siphon-runtime/src/runtime/{events,builder}.rs`, `crates/siphon-runtime/tests/runtime.rs` | Close just the affected waiter and verify its handler is dropped while the runtime continues to run. Preserve the partial-reply stop path. |
| `crates/siphon-ffi/tests/c/request.c` | Add the linked C regression. Wait for delivery, then complete the requesting process, check one `Closed` and one release, and check that the old reply is refused. |

Extensions cannot observe the transaction's final view or close the native handler's receiver. Verification
uses the macOS ABI 3 archive and its existing independent C and Rust consumers, with the private Beak and
Siphon workspace checks. Public package files, exported Siphon ABI and storage format stay the same.

Private commit `474a338`. The old ABI 3 archive queued the late reply and emitted no closure. The new archive
emits one `Closed`, releases the watch, and returns status 3 for the late reply while the runtime remains
open. Full Beak and Siphon workspace tests and Clippy, the three independent C consumers, all 13 tests of the
public Rust consumer, and Octoii's library and dependency checks passed on macOS ARM64.

## R05: reserve and finish stop phases through the scheduler

An independent C probe counted 249 stop phase invocations before it submitted any reply. The scheduler
checks the wait for ordinary work and skips it for stop phases. It also refuses a normal result after
termination is requested without releasing that run's admission, leaving the process Running until grace.

Give each reserved run its reservation commit sequence in addition to its process creation sequence.
Keep its mode and sequence with the scheduler's live admission. A finish must match that admission under
the same writer lock that applies its ending and changes. A normal run already executing when it is asked
to terminate releases its admission on finish, preserving the last committed checkpoint and discarding the
staged result. The scheduler can then admit the stop phase. A repeated completion for an earlier admission
must fail even if a later phase of the same process is already running. The existing on-disk record still
describes recovery after a host restart. The admission belongs to the current host session.

Clear the ordinary wait when termination is first requested. After the first stop phase, the existing
mail, change, child and deadline waits drive subsequent stop phases. The grace deadline is independent of
those waits and can still end a process that has no progress. Verify first admission, delayed partial answers,
grace while waiting on a child, and a normal completion racing stop. Run the linked C scenario with its reply
held until the first stop phase has registered its wait.

Confirmed private scope, relative to `devxiyang/octoii-siphon`:

| Files | Change |
| --- | --- |
| `beak/crates/beak-process/src/{commit,scheduler,lib}.rs`, `beak/crates/beak-process/src/scheduler/simulation.rs`, `beak/crates/beak-process/tests/scheduler.rs` | Define and check the run admission, release it on finish, and schedule both kinds of phase through the existing wait logic. Migrate the scheduler tests and simulator to use real admissions. |
| `beak/crates/beak-volume/src/{lib,volume}.rs`, `beak/crates/beak-volume/tests/volume.rs` | Route completion through the admission check before applying the write batch. Test handoff, duplicate completion, and preservation of the last committed checkpoint. |
| `beak/crates/beak-c/src/kernel.rs`, `beak/crates/beak-c/include/beak.h`, `beak/crates/beak-c/tests/abi.rs`, `beak/crates/beak/src/kernel.rs` | Carry the reservation sequence over the private Beak boundary. |
| `crates/siphon-runtime/src/changes.rs`, `crates/siphon-ffi/tests/c/request.c` | Preserve the forced-removal test for an admitted run and verify delayed partial replies with the linked ABI 3 consumer. |

Only Beak can validate and release its run admission or decide when a sleeping stop phase is runnable.
The storage encoding and the public Siphon ABI need no change for this repair.

Private commit `9fe5c4c`. The linked C regression withheld the answer until the first stop wait was
committed and another process had finished. The old archive re-entered the stop phase before the answer;
the new archive waited for the answer and then committed the partial result. The scheduler also verifies
that grace remains due while a child is still running and that the last descendant wakes its stopping
ancestor across a directory. On macOS ARM64, all 148 Beak tests, 53 Siphon tests, three linked C consumers,
and 13 public Rust consumer tests passed, along with strict Clippy and Octoii's library and dependency checks.

## R06: make the transaction boundary authoritative for execution

The current step drains runnable work before its transaction succeeds. After the root commits, scheduler
observation reads the journal and process records and can fail before the Volume publishes that commit.
The dispatcher logs a finish error and drops the already evaluated result. These paths need one answer
to whether a run was admitted or finished.

Prepare the scheduler's next view from the same transaction that prepares the tree and its change set.
The commit installs the tree and scheduler view, consumes the admitted work, and publishes committed notices
under the existing writer lock. Retry a reservation or an already evaluated result only after a known abort.
Use the exact admission from R05 and never re-run its program to recover a write failure. Distinguish an
indeterminate append from a known abort and fault the runtime for the former until recovery can establish
the authoritative state. Account run time up to the evaluation's completion, independently of transactional
retry delay. Verify each failure boundary with storage fault injection.

Implement this in dependency order:

1. Prepare the final tree change set and scheduler projection before publishing either. Preserve ready work
   if preparation or the commit aborts. Apply the projection and publish notices after the one successful
   commit. The scheduler uses the prepared tree rather than reading its own committed journal back.
2. Have storage distinguish a proven abort from an indeterminate append. An append that may have written
   part or all of its frame faults the writer until recovery resolves it. Carry these outcomes across the
   native Rust Beak interface to the runtime.
3. Keep the evaluated result and its wait context until its admission is finished or invalidated. Retry the
   stored result and queued answers only when the failed commit is known to be unapplied. A terminal fault
   stops the runtime and is available to the host through its next operation. Retained snapshots are still
   snapshots of their last confirmed state.

### R06a: prepare the tree and scheduler together

On private commit `29c96e8`, the simulated disk reproduced two failures. A rejected write left the process
`Runnable` at commit 1, but the next step returned no run. Another step returned an error after commit 2 had
already moved the process to `Running`. Both steps had consumed work that the host could no longer obtain.

1. The tree prepares one change set, pending root and indexes. The prepared transaction owns the objects
   that have yet to be written. Its read interface serves the affected process records, removal paths and
   next expiry from that same view. A direct tree commit uses the same preparation and persistence path.
2. The scheduler collects the data it needs from the prepared view, in change-set order. It stages the
   affected entries and notices while retaining the current live table and wait queues. A preparation
   failure or a known write abort leaves the existing root and the selected wakes available to the next step.
3. After the device accepts the commit, consume the wakes selected by this step, then apply the updates
   and any wakes produced by the commit. This ordering preserves a wake that is both consumed and raised
   again in one step. Install the corresponding run admissions and publish the snapshot and notices
   under the existing Volume writer lock. Applying the scheduler updates uses the prepared data in memory.
   Journal pruning remains best-effort maintenance.

Confirmed private scope, relative to `devxiyang/octoii-siphon`

| Files | Change |
| --- | --- |
| `beak/crates/beak-tree/src/{lib.rs,tree.rs,tree/transaction.rs}` | Expose the prepared read view within Beak and share the prepare and persist path with direct tree commits. |
| `beak/crates/beak-process/src/scheduler.rs` | Prepare ordered updates for affected paths and consume the selected wakes after persistence. Reuse the existing admissions, live table and wait queues. |
| `beak/crates/beak-process/tests/scheduler.rs` | Reject an unreadable removal during preparation. Use the simulated disk to verify that a committed reservation returns its admission even when the journal read fails. |
| `beak/crates/beak-volume/tests/volume.rs` | Use the existing memory device to fail a reservation commit. Verify the snapshot, notices and readiness, then reserve once on retry without issuing another wake. |

The tree and scheduler own this commit boundary. Extensions observe notices after it. Verify the existing
wait, cancellation, stop handoff and process recovery suites, then link the ABI 4 C and Rust consumers.
R06b owns ambiguous storage outcomes. R06c owns retrying a completed phase after a known abort.

Private commit `13ca006`. All three fault regressions failed on the baseline and passed after the change.
On macOS ARM64, Beak passed 147 tests including the scheduler simulation. Siphon passed 23 tests, the
three independent ABI 4 C consumers and 13 public Rust tests passed, and Octoii passed 105 library
tests with its live model test ignored. Node's all-target check, formatting and Beak/Siphon Clippy passed.

### R06b: resolve uncertain log appends before writing again

The simulated disk reproduced two problems on private commit `13ca006`.

- A disk that wrote the whole append and then returned an error left the open device at root 1. Reopening
  found root 2. A second commit on the original handle could overwrite that complete, unacknowledged commit.
  With a short write instead, reopening found root 1.
- Recovery cleared the first damaged record and left a valid old commit beyond it. A new append filled the gap.
  On the second reopen both commits appeared at sequence 2, and the old one gave the device its root.

The active log segment owns both the append position and whether it can still be written. A failure preparing
space leaves the committed root untouched and returns `WriteAborted`. Once the device attempts a write, an
error provides insufficient evidence to reuse that position. Record the first failure and return
`ReopenRequired` from the append and all subsequent writes, collection and sync at the device. Reads continue
from the last confirmed root and indexes. The same rule covers collection copying objects to the active log.
A segment rollover that fails after a successful append still leaves that commit accepted. A positioned write
can report an error after an earlier portion was written.
[Rust `FileExt::write_all_at`](https://doc.rust-lang.org/std/os/unix/fs/trait.FileExt.html#method.write_all_at)

Reopening scans the existing segment, retains its valid prefix and truncates the entire unverified tail.
A complete commit record determines the recovered root. The existing Tree open sync makes that result durable
before the scheduler starts. The recovered sequence and journal let the host reconcile what happened before it
submits another operation. Reopening the same directory requires releasing handles that still hold its lock.

The Beak Rust interface carries these two outcomes through its existing error types. Siphon's existing request
answer retry waits and retries only on `WriteAborted`. Other errors follow its existing terminal path.
R06c covers retaining completed Fiber results through retry and publishing terminal runtime failures to the host.
This batch uses the existing segment format and public ABI 4.

Confirmed private scope, relative to `devxiyang/octoii-siphon`

| Files | Change |
| --- | --- |
| `beak/crates/beak-storage/src/{lib.rs,device.rs,memory.rs}` | Define the two outcomes and the device contract. The memory test device reports a proven abort. |
| `beak/crates/beak-storage/src/{log.rs,log/record.rs}` | Fence mutations at the active segment, rebuild from its valid prefix and remove the old partial-clearing logic. |
| `beak/crates/beak-storage/src/disk/simulated.rs`, `beak/crates/beak-storage/tests/log.rs` | Inject a one-shot error after a partial or complete write to the active log. Verify the fence, recovery, preallocation failure and rollover. |
| `beak/crates/beak-tree/src/tree/simulation.rs` | Reopen after an uncertain append in the model test, then compare the recovered tree with the expected commit. |
| `beak/crates/beak-volume/src/{lib.rs,volume.rs}`, `beak/crates/beak/src/lib.rs` | Carry the two storage outcomes through the Rust interface and document what an error means for snapshots and notices. |
| `crates/siphon-runtime/src/runtime/events.rs` | Restrict the existing request answer retry to proven aborts and test the error paths. |

Check that a preallocation failure permits the same commit to retry. Inject short, complete and damaged-tail
writes, then verify the live root, journal and indexes, the immediate rejection of write/sweep/sync, and the root
and ordered journal after recovery and a later append. Re-run the Beak and Siphon suites and the linked ABI 4
consumers. An extension sees only the published state and errors after Beak has attempted the write. It cannot
resolve the log tail or stop other mutations on the same writer.

Private commit `3627372`. Fault injection covered preallocation, partial and complete appends, a damaged log
with a complete stale record after the break, and collection's copy into the active segment. The Beak Rust
interface and Siphon answer retry tests verified the error classification. On macOS ARM64 with Rust 1.100,
Beak passed 152 tests, Siphon passed 23 tests, and the linked ABI 4 consumers passed three C cases and 13 Rust
tests. Octoii passed 105 library tests with the live model test ignored. Node's all-target check, the Rust
dependency check, formatting and strict Beak/Siphon Clippy passed.

## Command cleanup and binary streaming

Beak's command cleanup currently treats a failed record lookup the same as a missing record and can kill a
live command on a storage read error. Send the signal only when the committed state establishes that the
command record is gone. Cover failed lookup and confirmed removal separately.

B2.2c should reuse request identity to scope retained stream events and human answer admission. The current
source wrapper can merge streams of different processes that supply the same mailbox key. Cover distinct
deliveries with that key before exposing streams over the public ABI. Keep reusable Cargo compilation data
under the [local verification cache policy](../CONTRIBUTING.md#local-verification-cleanup).

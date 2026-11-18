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

Before R06, a step drained runnable work before its transaction succeeded. After the root committed,
scheduler observation could fail while reading the journal and process records, before the Volume published
the commit. The dispatcher logged a finish error and dropped the evaluated result. These paths need one
answer to whether a run was admitted or finished.

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

### R06c: retain a completed phase until Beak resolves its commit

An isolated run against private commit `3627372` exercised the real log on macOS. Denying file growth
during finish left the process `Running` at sequence 2 with no output. The Fiber had run once, and the host
could commit again once the limit was lifted. Interrupting the append instead left the same published state
and made writes return `ReopenRequired`. The runtime still issued IDs and new snapshots. Commit watchers
also stayed open after shutdown. In a successful run, advancing the clock from 1000 to 1075 during evaluation
still produced `run-ms = 0` and `ended_at = 1000` because finish reused the reservation time.

The dispatcher owns the reservation until the phase is evaluated, then owns its result until Beak accepts or
invalidates that reservation. Keep the original Beak run, result or thrown error, staged changes, and versions
observed for its waits. On `WriteAborted`, retry the same finish with backoff and let Beak recheck the
admission. Request handlers retain their answer in the same way. A closed request or obsolete admission
releases the result. A successful finish may still hand off to the stop handler using the committed checkpoint,
as R05 defines. Transaction retries only repeat the Beak operation. A Fiber's registered retry policy still
applies when Beak commits a thrown ending. While the dispatcher is waiting to finish a phase, it holds at most
one evaluated result. Before evaluating a later reservation in that batch, it asks Beak whether the exact
admission still stands.

Measure phase duration with a monotonic clock around evaluation, before constructing or retrying its commit.
Pass that duration through Beak's existing finish path, including stop handoff and thrown endings. Keep the
commit clock for journal timestamps, outcome publication, deadlines, and retry scheduling. This preserves the
existing persisted `run-ms` counter and excludes both queue time before evaluation and storage retry delay.

Use the existing runtime shutdown path for terminal failures. The first terminal error is recorded before
signalling shutdown. `ReopenRequired` from a runtime storage operation and any unexpected failure that prevents
the dispatcher or an answered delivery from progressing take this path. Ordinary caller validation errors and
proven write aborts are returned to the caller. Use the existing task tracker for admitted blocking work so
shutdown waits for calls already in flight, even if their async caller was dropped. Reject work that reaches
admission after shutdown. Stop the dispatcher, request deliveries, commit watches and stream followers.
[Tokio's blocking task shutdown contract](https://docs.rs/tokio/latest/tokio/task/fn.spawn_blocking.html)

Subsequent runtime operations such as taking a snapshot, committing, or minting an ID return the original
fault. Retained snapshots remain readable. The native worker returns the fault on an operation or explicit
close through ABI 4's existing failure status. The host releases handles before reopening the volume and
letting Beak establish what was committed. A reopened runtime starts from Beak's recovered checkpoint under
the existing crash recovery rules. Callers holding raw Beak handles manage them through Beak's lifecycle.

Confirmed private scope, relative to `devxiyang/octoii-siphon`

| Files | Change |
| --- | --- |
| `beak/crates/beak-process/src/{commit.rs,scheduler.rs}` | Make measured phase duration an explicit input to finishing the exact admission. Apply it to the phase, its ancestors and the existing limits. |
| `beak/crates/beak-process/src/scheduler/simulation.rs`, `beak/crates/beak-process/tests/scheduler.rs` | Migrate direct finish calls and verify charging the process and its ancestors. |
| `beak/crates/beak-volume/src/volume.rs`, `beak/crates/beak-volume/tests/{volume.rs,concurrency.rs}` | Pass measured time through the transaction boundary. Verify that retrying after a proven abort charges it once and publishes the result once. |
| `beak/crates/beak/src/kernel.rs`, `beak/crates/beak/tests/beak.rs` | Pass the duration through the native facade and migrate its direct caller. |
| `crates/siphon-runtime/src/{fiber.rs,changes.rs}` | Separate evaluation from finish, retaining encoded output, wait observations and measured duration. Migrate the direct finish call in the changes test. |
| `crates/siphon-runtime/src/runtime.rs`, `crates/siphon-runtime/src/runtime/{builder.rs,dispatcher.rs,events.rs}` | Share write-abort retry and shutdown, own the pending result in the dispatcher, track admitted blocking work, and return the first failure through runtime operations. |
| `crates/siphon-runtime/src/streams.rs`, `crates/siphon-runtime/tests/runtime.rs` | Close followers with the runtime. Verify single evaluation, retry cancellation, fault visibility and draining. |
| `crates/siphon-ffi/src/runtime.rs` | Have explicit close join the worker and return its recorded failure. Runtime calls use the same error through their existing result path. |

Validate with injected aborts before reservation, finish and answer writes. Verify that finish retries neither
reevaluate the Fiber nor refresh its observed wait versions. Cover cancellation, stop handoff and obsolete
admissions. A faulted runtime stops admission, returns the same cause on subsequent host calls, and leaves prior
snapshots readable. Repeat the isolated disk fault probe and link the ABI 4 C and Rust consumers.

Extensions only supply Fiber code and request handlers. They cannot retain an admission's result after
evaluation returns, change how Beak charges it, or stop and report failure across runtime entry points.

Private commit `83d5fd6`. The isolated disk probe saw one Fiber evaluation and two finish attempts after a
proven abort. A short append stopped the runtime, closed its commit watch, and recovered the last confirmed
prefix on reopen. The linked C fault probe received the same error from snapshot, commit and explicit close
while retaining a readable prior snapshot. Unit tests also covered a wait version observed before retry,
a revoked queued admission, a caller dropping an in-flight blocking call, and request answer cancellation.

On macOS ARM64 with Rust 1.100, Beak passed 153 tests, Siphon passed 28, and Octoii passed 105 library tests
with the live model test ignored. Three regular C consumers, the C fault probe, and 13 Rust consumer tests
passed against ABI 4. Formatting, strict Beak/Siphon Clippy, Node's all-target check, and the Rust dependency
check passed.

## Identity across the commit boundary

Prepare decisions from a transaction's final view. Publish them after the append is confirmed. Each action
that arrives later carries the identity it was authorized against, and the owner checks that identity before
applying the action. R05 and R06 already enforce this for finishing a Fiber. Apply the same rule to command
records and requests using the existing tree, scheduler, and journal.

Beak owns committed process and request state. The Beak kernel owns command registrations, and the machine
owns the live operating-system handles. Siphon owns request delivery, cancellation and retained output. Octoii
and the binary bindings carry request identity through to the caller that will answer it. An append with an
unknown result follows R06b recovery. A delayed action whose target has been replaced is refused or discarded.

Volume identity bounds commit sequences. Synchronizing a volume can transfer its committed records and
checkpoints. A live command handle belongs to the machine that launched it. A retained request stream belongs
to the running Siphon instance. Reopening uses the recovered records and the existing request interruption
policy. The machine chooses its local process-control mechanisms for each supported platform.

## R07: bind command cleanup to the committed record

An isolated probe on private commit `83d5fd6` corrupted just the directory containing a live command's
record. An unrelated write still committed. The ensuing lookup returned `DamagedObject` and cleanup sent
`SIGKILL` to the live command. Replacing the entire parent with a file returned `NotADirectory` and correctly
stopped the command. Replacing the record directory itself with a file left the command running. A second
probe replaced the command directory created at sequence 2 with a new directory created at sequence 3 at
the same path. The old command stayed live and its later exit deleted the replacement directory.

When publishing a command record, create it at a vacant path and bind the held registration to that path and
creation sequence. A command awaiting its initial commit has no confirmed record yet. Once published, keep it
registered while that exact directory exists. A missing entry, a file in the path, or a different directory creation
sequence establishes that the original record is gone. A storage or decoding failure leaves that question
unresolved for a later check. Only confirmed removal or replacement authorizes the cleanup signal.
Take the committed snapshot under the command-table lock, preserving the publication ordering from B1.

Apply the same identity check inside the transaction that handles command exit. Delete only the exiting
command's record. A replacement file or directory stays where its commit put it. The snapshot and
transaction reads provide the evidence for these two operations. If the per-commit scan needs tuning,
use the existing change set to narrow affected paths and resolve the final record identities. A deletion
in the change set can be followed by a write to the same path in that commit.

Confirmed private scope, relative to `devxiyang/octoii-siphon`

| Files | Change |
| --- | --- |
| `beak/crates/beak-kernel/src/lib.rs` | Bind a held command to the exact record created by its publication. Use that identity in committed-snapshot reconciliation and conditional deletion on exit. |
| `beak/crates/beak-kernel/tests/kernel.rs` | Verify that a read failure keeps the command live. Replace the parent or record with a file, and replace the record with a new directory at the same path. Verify only the old command stops and the replacement survives exit cleanup. |

Run the kernel concurrency regression and the existing confirmed-removal case with the new cases. Recheck
Beak and the Siphon consumers that use the same kernel. Extensions run outside this reconciliation and
cannot intercept the signal once Beak's commit listener sends it.

Private commit `11d81af`. The four new kernel cases covered storage read failure, a file replacing the parent,
and a file or new directory replacing the command record. The kernel concurrency test passed ten consecutive
reruns. On macOS ARM64, Beak passed 157 tests, Siphon passed 28, and Octoii passed 105 library tests with one
live model test ignored. Three C consumers and 13 Rust consumer tests passed against the linked ABI 4 archive.
Formatting, strict Beak/Siphon Clippy, Node's all-target check, and the Rust dependency check passed.

### Command startup and recovery

`Kernel::spawn` currently launches the command before committing its record. `stop_left` uses the recorded
boot and numeric group for orphan cleanup. Exit cleanup attempts one deletion and discards its commit error.
Audit these as the next command lifecycle boundary for binary machine operations. The launch path needs
an exact owner and a release point for execution after record acceptance. Recovery needs proof that any
local execution it stops is the one that record names. Known write aborts and uncertain commits follow R06b's
different outcomes. Verify crash and teardown behavior per platform in Beak Machine.

## B2.2c: request identity for streaming and human answers

Probes on `83d5fd6` reproduced defects in the private source interfaces. Two live deliveries from
different processes published to the same topic with the same mailbox key. Both live delivery and replay
combined their offsets into 0 and 1. In a separate probe, a human answer validated against one process
incarnation committed to its replacement at the same path and mailbox key. When one process posted the
same pending key again, the request directory retained its creation sequence and the first handler could
answer the replacement.

Use the existing mailbox key as an address within its process. A request identity consists of the process path,
its start sequence, the mailbox key, and the request directory's creation sequence. Posting the same pending
key in a later commit replaces the request with a newly created directory. The transaction's final view determines
the single request that opens at that address, and the committed notices close the old request before delivering
the replacement. This reuses the tree's existing creation metadata. Multiple staged posts at one address in a
transaction yield one delivery of the final request.

Carry that same identity through delivery, stopping, closure and checking an answer at commit. A successful
answer writes the mail and clears the exact pending request atomically. The host reply handle and the human
answer both use this operation. The generic mail path currently also clears a pending request of the same key.
Reserve a pending request's answer slot for the identity-checked operation so ordinary mail cannot consume it.

Carry the identity with the human prompt and its schema from a single snapshot through to the client response.
The current Octoii thread answer scans by mailbox key. That boundary also needs the request identity so an
answer collected earlier can be rejected after the request is replaced. Siphon uses the same identity for
retention, live and replay offsets, deduplication, and ending a stream. Topics continue to route streams to
subscribers.

### B2.2c.1: bind request transactions and host replies

A probe on `11d81af` found three more consequences of sharing the mailbox address. Two posts to the same
key in one transaction emitted two deliveries, although the committed tree held only the final payload.
Reusing the key after an answer left that answer readable alongside the new pending request. A plain mail
to the pending key wrote a new value into the slot and closed the request.

Beak defines `RequestId` from the path, process incarnation, mailbox key, and request creation sequence.
Use the existing directory creation sequence as the generation. Posting a replacement recreates the request
directory and opens its answer slot empty, within the same transaction. Beak's pending request reserves the
slot for its matching answer. A plain mail to a reserved slot receives a refusal, and ordinary mail to other
slots keeps its existing semantics. During reopen, reconstruct identities from the existing directory data.
Use the same slot rule to resolve a recovered pending request with old mail at its address before retelling it.

Use the existing final-view notice preparation. Close a previously committed identity if it no longer matches
the final state. Deliver the final instance at most once per address, in staged publication order, and carry
its identity in interruption and close notices. A replacement closes its predecessor before it is delivered.
The transaction applies an answer only while the exact request is pending. Writing its mail and removing its
request directory are one change. If an append aborts or its outcome requires reopen, use R06's write contract.

The Beak snapshot exposes a pending request with its identity, name and payload from one view. Siphon uses
the identity carried by the Beak event for host delivery, cancellation and answer commit. For the current
address-based human answer call, take the request and schema input from one snapshot and carry the captured
identity through validation into the commit. This closes the race between validation and commit. Client
prompts still need to carry the identity of the request the person actually saw, in the next slice.
The existing host ABI reply handle remains scoped to its delivery and uses the same Rust answer path.

Confirmed private scope, relative to `devxiyang/octoii-siphon`

| Files | Change |
| --- | --- |
| `beak/crates/beak-process/src/{commit.rs,lib.rs}`, new `beak/crates/beak-process/src/request.rs` | Define request identity and the pending request view. Make publication, close, mail reservation and exact answer checks part of Beak's transaction. |
| `beak/crates/beak-process/src/scheduler.rs`, `beak/crates/beak-process/tests/scheduler.rs` | Reconstruct and retell the recovered identity. Test same-commit coalescing, old mail, replacements, ordinary mail refusal, late answers and recovery. |
| `beak/crates/beak-volume/src/lib.rs`, `beak/crates/beak-volume/tests/volume.rs` | Re-export the request types and verify the notice and answer path through Volume, including a rejected write. |
| `beak/crates/beak/src/{lib.rs,kernel.rs,snapshot.rs}`, `beak/crates/beak/tests/beak.rs` | Carry typed identity through the native events, snapshot and answer change. Migrate the native consumer tests. |
| `crates/siphon-runtime/src/{changes.rs,runtime.rs}`, `crates/siphon-runtime/src/runtime/events.rs`, `crates/siphon-runtime/tests/runtime.rs` | Key host deliveries by the native identity and apply the exact reply. Bind human schema validation to the same request snapshot and test both answer races. |

Verify one final delivery for repeated posts in a batch. Verify that normal replies, old host replies, ordinary
mail and a replacement racing schema validation affect only the intended slot. Verify close-before-deliver,
failed writes without notices, and reopen retelling once with the original identity. Run the existing binary
request consumers against the same ABI 4. Extensions see a request only after Beak has prepared and committed
its transitions. They cannot enforce the answer-slot invariant or reject a stale reply inside that transaction.

Private commit `68d010a`. The Beak tests cover final delivery, stale replies, reserved slots, aborts, and the
original request identity across reopen. The Siphon tests cover a late host answer and a replacement during
human schema validation. The Octoii recovered approval fixture now gives its answer through the runtime,
after the approval request commits, and still checks that an unavailable explorer tool cannot run or grant access.
On macOS ARM64, Beak passed 161 tests, Siphon passed 30, and Octoii passed 105 with one live model test ignored.
Three C consumers and 13 Rust consumer tests passed against ABI 4. Formatting, scoped strict Clippy, Node's
all-target check, and the Rust dependency check passed.

### Following slices

Next, migrate Siphon's retained and live streams and Octoii's human prompt and answer flow to the same
identity. Update the Rust and TypeScript protocol callers together. Keep the existing business keys available
for matching tool calls and timeline entries. Carry the exact identity through stream replays, close events,
pending form state, and the answer submitted by the client. After verifying those source paths together,
expose the verified stream and human answer interfaces through the public binary. Reuse Cargo compilation
data under the [local verification cache policy](../CONTRIBUTING.md#local-verification-cleanup).
The [cutover plan](SIPHON_CUTOVER.md) schedules this work alongside the remaining public capabilities,
platform artifacts, and removal of the product's private source dependency.

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

## Following repairs

- **Committed delivery closure and stop.** Bind live host deliveries to request identity and close their
  waiters when the request is removed later. Preserve cooperative stop and partial replies. Make stop phases
  honor waits and hand off from an already-admitted normal phase without running both. Verify delayed replies,
  grace expiry, terminal cleanup and no hot loop.
- **Storage errors around admission and finish.** Restore unconsumed ready work on a failed pre-commit
  reservation. Keep a computed phase result for retry only when a failure is known to be before commit. The
  scheduler also has fallible observation *after* the tree commits, so a generic Failed status cannot authorize
  replay. Distinguish uncertain or post-commit failure and surface unrecoverable faults to the host. Test the
  edges with an injected storage fault. Keep admission time and finish time distinct when addressing accounting.

After these repairs, use the same request lifecycle for B2.2c streams and human replies; verify human admission
against the actual request identity before exposing it across the ABI. Keep reusable Cargo compilation data
under the [local verification cache policy](../CONTRIBUTING.md#local-verification-cleanup).

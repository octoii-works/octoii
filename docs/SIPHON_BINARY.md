# Siphon public binary interface

2026-11-18. Agreed goal: Octoii depends on the published octoii-siphon-lib library and public
octoii-ai package; building and running Octoii requires no private source access.

## Repository ownership

- devxiyang/octoii-siphon (private): Beak source and Siphon mechanisms, their tests, binary build pipeline.
- octoii-works/octoii-siphon-lib (public): safe Rust wrapper, ABI declarations, documentation and versioned
  platform binaries. Private implementation source must not enter this repository or package.
- octoii-works/octoii: Agent behavior, tools, extensions and applications.

The public crate is named octoii-siphon-lib (Rust import octoii_siphon_lib). Native Rust ABI/rlib
is not the distribution boundary. Begin with a narrow C ABI and static linking, following Beak's
existing opaque-handle approach. Rust traits/generics stay on the wrapper side; no Rust ownership,
Future, trait object or unwind may cross the ABI directly. Concrete ABI details need implementation review.

## Implementation order

1. B1: import the complete Beak source into beak/ as an independent Cargo workspace in the private Siphon repository.
2. B2: design and validate one real binary execution path: open, register a program, spawn, call a host
   handler, commit, cancel, close/reopen and recover. Preserve existing durable execution semantics.
3. B3: converge product access to controlled storage, machine/sandbox and authorization mechanisms through
   the public wrapper; remove direct Beak and siphon-runtime dependencies from Octoii and Node.
4. B4: publish tested binaries and wrapper, switch the product to the published crate and remove its
   Siphon source submodule. Validate from a clean checkout without private repository credentials.

Each core implementation batch requires a concrete file list and separate confirmation. B1 permission
is not permission for an arbitrary ABI or new runtime implementation. Do not introduce another
Thread/Run/Job lifecycle or export every existing low-level API just to make compilation pass.

## B1 scope — implemented

The earlier nested-Beak-submodule trial is uncommitted and superseded by this source integration.
Import the crates from Beak commit 19d94a80ce10b7f37a65c03b521ead46dace32c1 directly under Siphon's
beak/ directory. Preserve its independent Cargo workspace, all 12 package boundaries and relative
dependencies. Rust/C source, tests, headers and package manifests are copied unchanged. Beak owns its
workspace defaults and lockfile; it does not inherit Siphon's workspace settings.

In private Siphon:

- Add every source path in the inventory below, relative to the private repository root.
- Replace the old vendor/beak distribution (14 previously tracked files); remove the uncommitted
  vendor/beak submodule/.gitmodules trial. No Beak submodule or extra CI credential remains.
- Cargo.toml: keep members = ["crates/*"], set exclude = ["beak"], set beak path to beak/crates/beak.
- beak/Cargo.toml and beak/rust-toolchain.toml: retain the independent workspace and align only the
  minimum Rust version/toolchain to 1.100 / 1.100.0. Preserve the remaining upstream configuration.
- beak/Cargo.lock: import the upstream lockfile; update only as required by the approved toolchain/build.
- Cargo.lock: resolve source-built Beak dependencies. No intentional unrelated upgrades.
- README and CI: explain source builds and verify both workspaces explicitly using --manifest-path beak/Cargo.toml.
  Siphon workspace tests do not implicitly run Beak dependency tests. No existing Siphon implementation edits.

In product Octoii: change its existing Beak dependency path, update Cargo.lock and the Siphon pin,
include the independent beak/Cargo.toml in the dependency checker and classify its packages as kernel, and update documentation. This migration
retains existing upper-layer accesses as tracked debt; it does not authorize new direct kernel access.

Beak's separate local checkout has an uncommitted toolchain change. Do not modify it. Preserve provenance
in the private repository; after migration, Siphon's internal copy is the maintenance source for this product.
Import all tracked Beak files, including its workspace manifest, lockfile, format/toolchain configuration
and distribution script. Do not import .git or target caches. Preserve independent dependency resolution:
building Siphon resolves its dependency graph using Siphon's lockfile; standalone Beak checks use Beak's lockfile. No nested submodule credentials are needed.
Extension-only changes cannot replace kernel source or workspace membership, hence core confirmation is required.

B1 validation: source inventory equality; source-built Beak with no old prebuilt .a or BEAK_DIST override;
fmt/Clippy, relevant Beak and Siphon suites, product regressions and Node build; private CI; commit/push private
Siphon first and then the product pin. If a source fix is required, seek a separate scoped confirmation.

## Platform requirement

macOS, Windows and Linux are mandatory binary release platforms. Define supported CPU architectures,
OS baselines and Linux libc/ABI explicitly before release. Each advertised target requires actual
execution, persistence/recovery, cancellation and permission tests, not only successful cross-compilation.
The current shipped Beak artifact only supports macOS ARM64; source integration alone is not proof of
other platform support. This requirement must be addressed before declaring binary distribution complete.

## B1 imported source inventory

Paths relative to octoii-siphon; their content comes from the pinned Beak commit:

```text
beak/.gitignore
beak/Cargo.toml
beak/Cargo.lock
beak/rust-toolchain.toml
beak/rustfmt.toml
beak/scripts/dist.sh
beak/crates/beak-c/Cargo.toml
beak/crates/beak-c/build.rs
beak/crates/beak-c/cbindgen.toml
beak/crates/beak-c/include/beak.h
beak/crates/beak-c/src/abi.rs
beak/crates/beak-c/src/change.rs
beak/crates/beak-c/src/command.rs
beak/crates/beak-c/src/failure.rs
beak/crates/beak-c/src/kernel.rs
beak/crates/beak-c/src/layout.rs
beak/crates/beak-c/src/lib.rs
beak/crates/beak-c/src/namespace.rs
beak/crates/beak-c/src/queue.rs
beak/crates/beak-c/src/snapshot.rs
beak/crates/beak-c/tests/abi.rs
beak/crates/beak-c/tests/c/host.c
beak/crates/beak-c/tests/c_host.rs
beak/crates/beak-kernel/Cargo.toml
beak/crates/beak-kernel/src/lib.rs
beak/crates/beak-kernel/tests/concurrency.rs
beak/crates/beak-kernel/tests/kernel.rs
beak/crates/beak-machine/Cargo.toml
beak/crates/beak-machine/src/access.rs
beak/crates/beak-machine/src/access/linux.rs
beak/crates/beak-machine/src/access/plan.rs
beak/crates/beak-machine/src/boot.rs
beak/crates/beak-machine/src/command.rs
beak/crates/beak-machine/src/lib.rs
beak/crates/beak-machine/src/output.rs
beak/crates/beak-machine/src/spec.rs
beak/crates/beak-machine/src/terminal.rs
beak/crates/beak-machine/tests/command.rs
beak/crates/beak-machine/tests/output.rs
beak/crates/beak-machine/tests/terminal.rs
beak/crates/beak-namespace/Cargo.toml
beak/crates/beak-namespace/src/file_system.rs
beak/crates/beak-namespace/src/host.rs
beak/crates/beak-namespace/src/inaccessible.rs
beak/crates/beak-namespace/src/label.rs
beak/crates/beak-namespace/src/lib.rs
beak/crates/beak-namespace/src/namespace.rs
beak/crates/beak-namespace/src/tree.rs
beak/crates/beak-namespace/tests/namespace.rs
beak/crates/beak-process/Cargo.toml
beak/crates/beak-process/src/commit.rs
beak/crates/beak-process/src/lib.rs
beak/crates/beak-process/src/record.rs
beak/crates/beak-process/src/scheduler.rs
beak/crates/beak-process/src/scheduler/simulation.rs
beak/crates/beak-process/tests/scheduler.rs
beak/crates/beak-signal/Cargo.toml
beak/crates/beak-signal/src/lib.rs
beak/crates/beak-storage/Cargo.toml
beak/crates/beak-storage/src/cache.rs
beak/crates/beak-storage/src/device.rs
beak/crates/beak-storage/src/disk.rs
beak/crates/beak-storage/src/disk/real.rs
beak/crates/beak-storage/src/disk/simulated.rs
beak/crates/beak-storage/src/hash.rs
beak/crates/beak-storage/src/lib.rs
beak/crates/beak-storage/src/log.rs
beak/crates/beak-storage/src/log/index.rs
beak/crates/beak-storage/src/log/record.rs
beak/crates/beak-storage/src/memory.rs
beak/crates/beak-storage/src/objects.rs
beak/crates/beak-storage/tests/disk.rs
beak/crates/beak-storage/tests/log.rs
beak/crates/beak-storage/tests/objects.rs
beak/crates/beak-sys/Cargo.toml
beak/crates/beak-sys/build.rs
beak/crates/beak-sys/src/lib.rs
beak/crates/beak-terminal/Cargo.toml
beak/crates/beak-terminal/src/draw.rs
beak/crates/beak-terminal/src/lib.rs
beak/crates/beak-terminal/src/palette.rs
beak/crates/beak-terminal/tests/screen.rs
beak/crates/beak-tree/Cargo.toml
beak/crates/beak-tree/benches/tree.rs
beak/crates/beak-tree/src/btree.rs
beak/crates/beak-tree/src/change_set.rs
beak/crates/beak-tree/src/codec.rs
beak/crates/beak-tree/src/directory.rs
beak/crates/beak-tree/src/files.rs
beak/crates/beak-tree/src/ids.rs
beak/crates/beak-tree/src/index.rs
beak/crates/beak-tree/src/lib.rs
beak/crates/beak-tree/src/page.rs
beak/crates/beak-tree/src/rows.rs
beak/crates/beak-tree/src/selector.rs
beak/crates/beak-tree/src/tree.rs
beak/crates/beak-tree/src/tree/simulation.rs
beak/crates/beak-tree/src/tree/snapshot.rs
beak/crates/beak-tree/src/tree/transaction.rs
beak/crates/beak-tree/src/working.rs
beak/crates/beak-tree/tests/killed.rs
beak/crates/beak-tree/tests/tree.rs
beak/crates/beak-volume/Cargo.toml
beak/crates/beak-volume/src/journal.rs
beak/crates/beak-volume/src/lib.rs
beak/crates/beak-volume/src/volume.rs
beak/crates/beak-volume/tests/concurrency.rs
beak/crates/beak-volume/tests/volume.rs
beak/crates/beak/Cargo.toml
beak/crates/beak/src/command.rs
beak/crates/beak/src/ffi.rs
beak/crates/beak/src/kernel.rs
beak/crates/beak/src/layout.rs
beak/crates/beak/src/lib.rs
beak/crates/beak/src/namespace.rs
beak/crates/beak/src/queue.rs
beak/crates/beak/src/snapshot.rs
beak/crates/beak/tests/beak.rs
```

## B1 validation findings

The independent-workspace source migration is authorized. Before the separately approved race fix, all 118 imported files matched Beak commit
19d94a8 except the two approved Rust version settings; no existing lockfile package version was removed.
The dependency checker recognizes 33 packages. Siphon source checks/tests and all workspace format checks pass.

Beak's existing kernel concurrency test failed twice (48 commands expected, fewer records observed),
with a passing run between failures. Inspection found that kill_unrecorded takes a volume snapshot
before locking the command map, allowing a newly recorded command to be examined against an older snapshot.
The user separately approved the fix in beak/crates/beak-kernel/src/lib.rs: lock the command map
before taking the snapshot and add a focused regression test in that same file. The new test fails
on the original implementation (unexpected SIGKILL), passes after the fix, and the unchanged original
concurrency test passed 20 consecutive runs. Beak full tests and strict Clippy pass after the fix.

B1 implementation is committed in private Siphon as aa73a09. Final local suites passed: Beak 141,
Siphon 45, and product 223 (2 ignored). Formatting, dependency boundaries and strict Clippy passed.
Private CI and final product pin verification are tracked during the migration. Public binary packaging
(B2–B4) remains separate work; the product still requires private Siphon source access.

The product's access suite initially failed once when reopening a just-closed volume: Tokio rejected
runtime destruction inside an async context. The isolated rerun, complete product rerun, and ten additional
access-suite runs passed. This intermittent failure is recorded, not claimed fixed by the command cleanup
change. Node binary build also passed. No access-test assertions or runtime lifecycle behavior were changed.

## B2 implementation and product exit criteria

The final integration must remove the product's siphon/ gitlink, .gitmodules entry, all Cargo path
references into siphon/, private checkout steps and SIPHON_DEPLOY_KEY use. Octoii depends on the
published octoii-siphon-lib package and crates.io octoii-ai. Repository creation alone is not completion.

### Boundary

The public package owns safe Rust types, serialization, typed Program/Request adapters, FFI declarations,
artifact selection and verification. The private binary owns durable state, scheduling, commit semantics,
request interruption/recovery, machine operations and the existing processing implementations.
Do not export Rust trait objects, Future, Tokio objects, references, Vec or String layouts across the ABI.
Use versioned C declarations, opaque handles, byte slices and explicit ownership/release functions.
Catch unwinding panics inside each side of a foreign call; never promise recovery from aborting panics.

Keep existing Program, Run, Changes, Wait, Request and Answer semantics. Runtime registration currently
uses associated constants and private erased traits; adapting those is a later explicitly scoped batch,
not justification for rebuilding the scheduler in the public wrapper. Run-scoped snapshots must keep
one consistent view, and a phase's changes and ending must still commit atomically.

The existing script, shell, image and text dependencies also require public replacement through this
package before deleting siphon/. Merely wrapping siphon-runtime leaves private dependencies in place.
No public Beak/kernel escape hatch is introduced to make product compilation pass.

### B2.1: binary lifecycle and storage slice — implemented, CI validation in progress

Start with a small executable slice before committing to the full callback protocol:
open a runtime, commit supported Changes, read a retained Snapshot, close, immediately reopen and verify
data. The runtime uses the existing Builder and public APIs; no new scheduler or storage implementation.
The C boundary owns its executor and destroys it safely even when called from a host Tokio task.
All failure paths, including open on an already locked directory, release owned resources without unwinding
across the ABI. Shutdown is explicit and distinguishes closing execution from releasing read handles.

Exact private scope (relative to octoii-siphon):
- Add crates/siphon-ffi/Cargo.toml (staticlib/rlib, unpublished workspace member).
- Add crates/siphon-ffi/src/{lib,abi,runtime,snapshot,changes}.rs and include/siphon.h.
- Add crates/siphon-ffi/tests/lifecycle.rs and tests/c/lifecycle.c.
- Update Cargo.lock, README.md, .github/workflows/check.yml for this slice and C-host verification.
- Existing runtime and Beak implementation files remain unchanged in this first slice.

Public work: establish octoii-works/octoii-siphon-lib; implement its matching safe wrapper, checked-in
FFI declarations, build configuration and contract tests. Consumer builds must not need bindgen/Clang
or private source. Before publishing, the package must carry immutable artifact identity and checksums,
reject ABI mismatches and unsupported targets, and fail clearly without a private-source fallback.

Why core work is necessary: only the private build can compile the implementation into a native library;
an extension or an alias crate would still require the private sources at consumer build time.

B2.1 is not the product migration. B2.2 adds typed program/request registration over the ABI and validates
phase execution, host answers, streaming, cancellation, shutdown and crash recovery. Its exact core files
will be separately reviewed after the first binary slice. B3 migrates the remaining machine/storage and
processing callers. B4 publishes the complete verified package and removes private checkout dependencies.

### Distribution verification

Initial target matrix: aarch64-apple-darwin, x86_64-apple-darwin, x86_64-pc-windows-msvc,
x86_64-unknown-linux-gnu. Add Linux ARM64 when native validation is available. Record OS/libc minimums
from the actual build environment before publication; do not infer them from the target name.
Static library symbol isolation and platform system libraries must be validated on each native runner;
the old Beak shell distribution script is not a Windows distribution implementation.

Release artifacts include target, interface/binary versions, SHA-256, license notices and platform minimums.
A clean public consumer CI job has no private checkout or credential and runs persistent recovery and
host-callback tests against the actual release binaries. Product CI must ultimately use that same path.
Do not publish a placeholder 0.0.1 that cannot serve the promised product interface.

References: Rust Reference, https://doc.rust-lang.org/reference/linkage.html;
Rustonomicon FFI, https://doc.rust-lang.org/stable/nomicon/ffi.html.

## B2.1 implementation record

Private Siphon commit 5447fa4 adds the approved siphon-ffi crate. Public interface source is at
https://github.com/octoii-works/octoii-siphon-lib (8d9cd8e). The wrapper uses only checked-in C declarations
and a verified native archive; it has no private source dependency or consumer bindgen step.

The implemented slice covers atomic mkdir/write/append/delete batches, retained snapshots and reads,
explicit shutdown, implicit shutdown on release, and persistent reopen. A dedicated host thread owns
the native executor, so opening/closing from a caller's Tokio task does not nest runtime destruction.
Snapshot handles may retain storage after runtime close and must be released before reopening it.
Directory paths currently require UTF-8. Calls are synchronous; async hosts should offload long operations.

Local validation: 47 Siphon workspace tests, strict Clippy and formatting; a separately linked C lifecycle
program; 3 public Rust integration tests covering persistence, snapshot consistency, atomic rollback and
10 asynchronous-host failed-open/drop/reopen cycles. Wrong target, ABI metadata and archive checksum are
rejected. Header copies match. CI builds an isolated-symbol static archive, runs the C consumer, checks
out the public wrapper at a fixed commit and runs its checks/tests using that binary alone.

This is an unpublished macOS ARM64 development artifact. SIPHON_LIB_DIR is an explicit local development
artifact input, not the final download/install protocol. The local manifest hash is not a release signature.
No binary release or crates.io version is published, no Windows/Linux support is claimed, and the product's
siphon/ submodule remains until B2.2–B4 replace all private dependencies. Program/request callbacks and
processing capabilities remain pending; a storage-only wrapper is not a complete Siphon distribution.

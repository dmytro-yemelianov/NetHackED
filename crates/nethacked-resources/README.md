# Optional M3b resource service

`nethacked-resources` is an optional dependency of a host, independent of every
NetHack domain crate. The kernel owns no resource data, clock, schemas, policies,
transaction coordinator or universal service container. `Service::install`
registers ordinary native `read`, `prepare`, and `commit` ports. Wasm callers
reach them with `kernel.call` and their declared dependency slots. Native
callbacks receive bytes and immutable `CallerView`, with no mutable dispatcher.

Trusted host administration constructs the service, installs it once, registers
immutable owner policies, and initializes/updates/deletes generic owner/key byte
cells. These APIs are not guest imports. Policies bind each owner name to a
particular capability and original provider handle/artifact. Guest requests
contain addresses/stamps/bytes or a transaction ID, never caller credentials.
Unknown top-level request fields refuse. Identity, loaded caller artifact, and
executing service incarnation come from the kernel. Owner names may differ from
provider module names; only the host policy defines that relationship.
The service's native identity includes the port ABI tag, package version and
canonical service limits. This declared native code identity still depends on
the trusted compiled-code fidelity assumption described below.

`read` requires the invoking module's live participant read permission, rather
than merely service admission. `prepare` and `commit` require write admission to
the coordinator, but readonly participants need only read permission. Kernel
inspection intersects each actual caller grant and manifest dependency with the
current call's access/scope ceiling, including all ancestor ceilings. A read port
cannot obtain a write permit. The coordinator's own grants never contribute.
Each permit records the exact original provider and complete ordered extension
resolution (handles, artifacts, export names). Commit computes fresh permits and
compares these bindings; unrelated grant changes do not invalidate a plan.

Plans declare unique stamped readonly dependencies, unique stamped writes
(replacement or whole deletion), and ordered event bytes. Owner validators see
only their declared subset plus proposed events. The accounting validator sees
only the declared multi-owner snapshot and exact plan. No resource lookup or
mutable service object is passed to validators. Host-provided predicates must be
sound, deterministic, terminating, and free of external side effects; captures
can defeat those assumptions, which Rust cannot prove. Any captured reentrant
service API/port access refuses through `try_borrow`/`try_borrow_mut`, including
inspection, rather than interleaving publication or panicking on borrow failure.

One `Rc<RefCell<Inner>>` mutable borrow covers authorization, validation, output
serialization and publication. Preparation stages the ID reply before allocating
an ID/pending entry. Commit stages canonical JSON and an entire replacement state,
including checked revisions, event capacity and commit counter, then assigns it
once and consumes the pending entry. There are no callbacks during publication.
All refusal paths precede authoritative mutations. Allocation failure/abort or a
trusted callback panic is outside the returned-refusal contract; callbacks run
before publication. Native CPU time is a trust assumption.

Limits bound owners, live cells, participants, pending plans, event counts,
history, each payload, and request/reply bytes. Names are limited to 128 ASCII
name bytes. There are no tombstones or unbounded address/identity tables:
deleted addresses can be reused only with a fresh global service generation.
Every monotone counter uses checked arithmetic. Bounded wire decoding precedes
shape validation. Pending storage is private and immutable; `State` is a cloned
inspection value including private pending contents for exact equality evidence.
History and pending exhaustion refuse until host policy changes or pending plans
commit; there is no eviction, cancellation, or persistence protocol here.

## Concrete Lean correspondence

This table describes the adapter to `NetMechanics/Authority.lean` definitions
1–224. It is an executable correspondence proposal with fixtures, not a Lean
refinement theorem. Architecture capability names are explicitly encoded as
`(provider module ID, endpoint, major)`; they do not become Authority capabilities
merely because both structures are called `Capability`.

| Lean definition | Concrete runtime representation / check |
| --- | --- |
| `Artifact`, `RuntimeTrust.currentCodeBinding` | Kernel SHA-256 artifact of code/native identity bytes, sorted manifest, ABI/kind, limits, and extension target artifact. Trusted native identity bytes must faithfully bind compiled callback code/configuration; cryptographic and native-code authenticity are assumed. |
| `Caller`, `authority` | Kernel-constructed `CallerView.caller()` instance-bound `ModuleHandle` plus `caller_artifact()`. The handle's private instance/generation identifies host/principal incarnation. Guest JSON cannot manufacture this view. Stored caller and artifact must equal the fresh commit view. |
| `Provider`, `permitted` owner/code/export tests | Trusted `OwnerPolicy` maps owner string to capability and original provider handle/artifact; `Inner::permit` checks that mapping and the live export resolution. This explicit owner-to-module mapping replaces Lean's numeric `provider.owner == address.owner`, rather than asserting a bijection between strings and principals. |
| Authority/Architecture `Capability` | Kernel `Capability { provider, endpoint, major }` is the concrete named/versioned capability. `CapabilityPermit.resolution` additionally binds every current extension handle/artifact/export, so original-provider equality alone is insufficient. |
| `Access`, `Grant`, `permitted` | Kernel live slot grant, target incarnation, manifest dependency, and invocation ceiling. Inspection checks the specific resource and requested read/write access. Runtime write includes read: its adapter corresponds to both Lean read and write grant entries; runtime read corresponds to only a read entry. Lean's exact grant-list membership is not itself an access-order proof. |
| `Address`, `Handle`, `Stamp`, `Cell`, `current` | `Resource { owner, key }`, service-global non-reused `Stamp.generation`, checked `revision`, and immutable `Snapshot.value`. Whole deletion removes the cell; trusted recreation consumes a new generation. |
| `Dependency`, `Write`, `Plan`, `participants`, `shape`, `snapshot` | Guest stamps plus service-resolved owner permits; `Plan.readonly`, `writes`, ordered `events`; unique address set and participant/payload limits. `Declared.snapshots` contains exactly present declared cells. Replacement/deletion can target only an existing stamped participant. |
| `Policy`, `valid`, `RuntimeTrust` | Immutable owner validators and accounting validator; host soundness/side-effect assumptions correspond to Lean `ownerSound`/`accountingSound`. Kernel authenticates invocation/provider and host issues grants. Finite bounds and checked overflow add refusal cases to Lean's unbounded `Nat` transitions. |
| `Identity`, `fresh`, `Prepared`, `prepare` | Process-unique service namespace plus checked serial, private pending map containing exact plan, caller/artifact, service binding and per-participant resolutions. Service namespace replaces the transaction host token; binding to kernel host is separately enforced by instance-bound caller/provider handles. Successful prepare changes only pending/serial. |
| `commitValid`, `patch`, `applyPrepared`, `commit` | Fresh view plus stored plan revalidation; staged complete state; one assignment under the service borrow publishes all owner patches and event append, increments commits and removes pending. Refusal preserves all service data/counters/pending; replay fails. |

The namespace allocator is injective within one running process and fails before
overflow. Kernel module handles are likewise process-instance-bound. Neither
claims non-reuse across restart, snapshot rollback, distributed hosts, or future
persistent codecs. Host owner predicates, native artifacts, hashing and Wasm
isolation remain trusted adapter obligations. No durable/distributed exactly-once,
composed workflow binding/pinning, schema migration, timer scheduler, or NetHack
migration is implemented. A refusal does not undo previously committed calls.

Runtime fixtures exercise real Wasm read/prepare/commit, separate readonly grants,
revocation/narrowing, stale stamps and generation reuse, provider/extension
replacement, identity mismatch/replay, policy failures, bounds, preflight and
reentrant access. The generic item/equipment/timer/debt fixture deletes the three
cells together while retaining quantity, terms, creditor and liability in the
consumed debt cell; unrelated data and an existing event prefix survive. Private
boundary fixtures cover counter overflow and fallible reply staging.

## Operation-owned prepared plans

Prepared plans capture the kernel's private optional `OperationBinding` and commit
rechecks exact equality alongside existing caller/artifact/provider/permit checks.
Same module code in another operation, epoch, phase or lease cannot adopt a live
plan. Bound and legacy unbound plans cannot adopt one another; legacy unbound
prepare/commit behavior remains available. Guest commit messages remain transaction
IDs only; forged JSON context is refused, not interpreted as host authentication.

Trusted lifecycle `discard_operation` discards only pending plans with the exact
binding; it leaves resources, history and counters unchanged and is idempotent.
The workflow typed participant adapter runs it after every invoked phase and
retains the operation's code/participant references on cleanup failure. Host wiring
must enroll each reachable phase-scoped service in the bounded participant list;
there is no runtime service discovery. See [workflow policy](../nethacked-workflows/README.md).

Each service exposes its private, non-reused typed cleanup identity to the trusted
host. Bound preparation additionally refuses if this identity is absent from the
kernel-authenticated participant set. The workflow freezes that set from enrolled
typed participants, so omission cannot silently leave abandoned bound plans.
Participant wrappers must forward the service identity and idempotent discard.

# M3b: optional atomic resource service

Implement the resource portion of [M3](2026-10-08-runtime-kernel-contract.md) against [S1/S3](2026-10-08-authority-transaction-contract.md). The [Astra audit](../parity/obligations/2026-10-08-architecture-astra-review.json) requires an explicit Lean/runtime identity and capability correspondence. M4 workflow binding/pinning/persistence and M5 NetHack remain later work.

## Ownership and authority

Create a separate optional `nethacked-resources` crate depending on `nethacked-kernel`. The kernel owns no resource state, game schema, clock or transaction coordinator. Use existing serde/serde_json dependencies; no new external dependency is needed. Trusted host APIs register owner policies and initialize/inspect generic owner/key byte resources. Native callbacks get bounded payloads and authenticated immutable views, never mutable Kernel/World or a universal service container.

Add only a bounded immutable capability-inspection surface to the kernel caller view if needed. It must derive from the actual invoking module's live manifest/grants and registered provider incarnations, intersect the current invocation's effective scope/access ceiling, and expose no dispatch or mutable registry. It must not use the coordinator's broader grants. Missing/revoked/stale/wrong-version grants cannot become permits. Participant permissions can differ: readonly dependency needs read access while a write needs write access. Owner policy metadata identifies which provider/capability protects that owner; the guest cannot invent the binding. Snapshot the actual provider artifact/extension resolution relevant to a permit so replacing provider code cannot silently adopt a prepared plan. A read endpoint cannot gain write permits by inspection. No global authorization epoch that unrelated grant changes invalidate unnecessarily.

Service read/prepare/commit are ordinary bounded native ports reached by real Wasm guest calls. Trusted host administration is separate. The service authenticates callers and its own provider incarnation from CallerView; caller identity/artifact is never supplied in guest JSON. Commit obtains a fresh view and revalidates the original caller, live per-participant grants, capability/provider identities and effective scopes. The read/prepare/commit endpoint admission grant alone is insufficient authorization for arbitrary owner data. No blanket write grant is required for readonly participants.

## Exact transaction behavior

Resources have owner/key addresses, non-reused generations and checked revisions. Whole deletion followed by reuse has a different generation. Bound live resources, identities/address bookkeeping, payloads, participants, pending plans and history/events; all counters use checked arithmetic and fail before mutation on overflow. All maps/output ordering are deterministic.

A plan contains readonly stamped dependencies, unique stamped writes (replacement bytes or whole deletion), and ordered bounded event bytes. Owners provide trusted side-effect-free validators over only declared snapshots and proposed changes; an accounting validator can examine that same declared multi-owner snapshot/plan. Validate provider/owner bindings, all permissions, shape/bounds/duplicate participants, generations/revisions and validators before allocating any pending ID. Reject hidden reads and undeclared writes by construction. Keep exact immutable plans and caller/provider bindings in service-owned pending storage. Preparation leaves resources/history unchanged; refusal leaves every service field, including counters/pending entries, unchanged.

Commit accepts only a service-bound transaction ID, resolves the stored plan, repeats all current validation, stages output serialization and all fallible checks before publication, then atomically updates all owners/history and consumes the pending plan once. No mutable-resource callbacks during publication. One serialized service borrow/critical section must exclude interleaving and reentrant mutation; trusted validator assumptions must be documented and reentrant access must refuse, not partially mutate. Replay, wrong host/caller/artifact/provider, revoked/narrowed grants, stale readonly/write stamps, deleted/recreated handles and invalid owner/accounting policies refuse with exact whole-state preservation. A transaction refusal does not erase earlier successful workflow effects. No durable/distributed exactly-once claim.

## Required evidence

- Real guest->kernel->resource ports exercise read, prepare and commit; callers cannot supply another identity or substituted commit writes/events.
- Multi-owner item/equipment/timer deletion and retained consumed-debt replacement preserve quantity/terms/creditor/liability, unrelated state and existing event prefix. This is a generic typed-fixture example, not a new NetHack domain implementation.
- Wrong caller/host/provider incarnation/artifact/capability version, revoked or narrowed participant grants, readonly vs write permissions, changed provider resolution, stale readonly/write revisions, generation reuse, duplicate participants, bad owner/accounting validation, replay and bad IDs all preserve exact state on refusal.
- Bounds/overflow, serialization/reply preflight and reentrant validation are covered with meaningful fixtures. New private unit fixtures may reach counter boundaries; existing tests/helpers/assertions remain unchanged.
- Explicit Lean correspondence table names the concrete mappings for Caller/Artifact/Provider/Capability/Grant, Handle/Stamp, Prepared/Identity/Policy and atomic publication. Explain trusted predicates, finite bounds and uncomposed workflow/persistence obligations accurately; similar type names are not a refinement proof.

Run focused package tests/clippy/fmt and explicit wasm32 builds of both kernel and resource crate; root handles independent semantic review, full12/all3 and commit. No rebaseline, domain migration, workflow/codec implementation or unrelated process changes in M3b.

## M3b concrete implementation correspondence

The optional [resource crate](../../crates/nethacked-resources/README.md) implements
ordinary guest-called read/prepare/commit ports. Its README records the complete
Lean definitions 1–224 adapter table and trust boundary. Concretely:

- `Caller`/`Artifact` are the fresh kernel `CallerView` instance-bound caller handle
  and loaded artifact. `Provider`/`Capability` use immutable host owner policy
  bindings to `(module ID, endpoint, major)` and exact ordered original/extension
  handle-artifact-export resolution. Owner strings map through policy to module
  principals; they are not asserted to equal Lean numeric principals.
- `Grant`/`permitted` map to the actual invoker's live slot grants, current provider
  incarnation and manifest dependency, intersected with every call ceiling.
  Each participant checks its own resource/read-or-write access. Runtime write
  corresponds to both read/write entries in Lean's exact grant-list model; read
  corresponds to read only. The coordinator's grants confer no participant rights.
- `Handle`/`Stamp` map to owner/key plus non-reused service generation and checked
  revision. `Plan`/`Snapshot` are bounded declarations and their exact old values.
  Owner validators receive only their declared subset; accounting sees only the
  declared multi-owner plan/snapshot. Soundness and external side-effect freedom
  remain trusted assumptions corresponding to `Policy` predicates.
- `Identity`/`Prepared` map to process-unique service namespace plus checked serial
  and private immutable pending plan/caller/service/permit bindings. Kernel host
  binding is enforced separately by private instance-bound module handles.
- `valid`/`commitValid` repeat fresh authorization, bindings, stamps, shape and
  validators. `patch`/`applyPrepared` are complete staged state publication under
  one exclusive service borrow, after checked counters/history and serialized
  reply bounds; no callbacks run during publication. Refusal preserves pending,
  counters, resources and history. Reentrant access explicitly refuses.

These checks and generic runtime fixtures are not a formal refinement proof.
Machine bounds add refusals to unbounded Lean counters. Native artifact fidelity,
owner/accounting soundness, kernel isolation and process namespace allocation are
trusted. Namespace persistence across restart/rollback, composed workflow
binding/pinning, durable/distributed exactly-once and NetHack migration remain
unimplemented. Independent review, full workspace gates/goldens and commit remain
root-owned follow-up work.

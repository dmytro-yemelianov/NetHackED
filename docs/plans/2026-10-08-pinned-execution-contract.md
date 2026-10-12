# M4a: retained executable pipelines and lifecycle references

Implement the execution-retention prerequisite of [M4](2026-10-08-microkernel-goal.md), following the [workflow](2026-10-08-workflow-contract.md), [lifecycle](2026-10-08-service-contracts.md), and [Astra findings](../parity/obligations/2026-10-08-architecture-astra-review.json). A4 remains pending until an authoritative workflow checkpoint actually suspends/resumes on these bindings. No persistence/codec or NetHack migration in this child.

## Retained execution

A trusted host captures a bounded, host-bound, non-reused opaque pipeline/lease identity for an actual callable and effective authority ceiling. The kernel owns its immutable executable routing description and actual code references; clients cannot submit replacement routes. Record the root callable, exact original providers, resolved ordered extension chains and transitively reachable declared capability endpoints, including native and guest providers. Traverse finite graphs with explicit limits; unsupported cycles or capture failure must refuse before publishing any lease or advancing counters. Do not assume retaining only the top-level artifact freezes nested providers. Document capture semantics for slots without an issued grant; no absent grant is permission.

Invocation through that lease executes its retained root and nested routes, even after new extensions change ordinary dispatch. Capability inspection presented to resource ports must describe the same retained routes that execution uses. All calls still check current caller registration, live slot grants, exact original provider incarnation, dependency version, request/response bounds, and ancestor scopes/access ceilings. Revocation/narrowing takes effect immediately; retaining code cannot freeze old authority or adopt replacement identities. A supplied invocation authority cannot exceed the captured ceiling. Preserve fuel/call/depth/memory budgets, ABI preflight and committed-prefix behavior on both ordinary and retained paths.

Ordinary new invocations use current dispatch. A lease records private host identity/serial and a code-binding descriptor/hash suitable for the later workflow/codec adapter. Stale, released, foreign-host and fabricated wire identities refuse. Releasing once invalidates that lease, removes only its references and preserves shared references held by other leases. No process-restart identity or schema/codec claim yet.

## Lifecycle boundary

All code referenced by a retained pipeline remains installed and unload attempts through **any** removal API must refuse while referenced. Support explicit bounded host-owned retention for future resource/schema/timer references without making those service types compulsory kernel fields. No global single-world context and no mutable kernel access in domain callbacks.

Provide dependency-aware managed detach/unload operations: detach changes future dispatch while retaining code for pending work; unload checks current dispatch, registered dependents and retained/owned references before removing code. Registration/attachment/detachment/release/unload are safe-boundary operations, enforced by actual exclusive access/active-call rules rather than guest declarations. Refusal must leave the complete registry, dispatch, grants, leases/references and identity counters unchanged. Do not add guest initialization side effects. Existing low-level registration/unregistration callers and tests are compatibility surfaces; preserve their currently tested unpinned behavior where stricter lifecycle requires a separate managed API, document the distinction, and enforce pinned-reference protection in the low-level removal path as well. New workflow/session hosts will use managed lifecycle APIs. Unsupported replacement/schema migration is refused or remains an explicit later service obligation, never silently guessed.

## Required fixtures

1. Populated entity-free host: capture old executable pipeline, change dispatch by attaching a real Wasm algorithm, show ordinary/new captures use new behavior while the retained pipeline uses old behavior and preserves unrelated state. Include nested guest provider/extension changes, not only root replacement.
2. Multiple leases sharing providers; foreign/released lease refusal; exact code identity/reference retention; detach permits old execution; release allows safe unload only after the last relevant lease/owner reference and dependent is gone. Failed capture/removal/limits preserve state and counters.
3. Live revoked/narrowed grants and readonly ancestor ceilings still refuse on retained nested paths. Budgets and ABI reply preflight remain effective; no authority gained through routing snapshots.
4. Real resource-service prepare/commit under a retained pipeline: changed global provider dispatch cannot rewrite its captured chain; ordinary changed-chain commit refuses while the valid retained operation can commit. Revocation still refuses with exact resource/pending/history preservation. This fixture supplies executable composition evidence, not yet operation-checkpoint ownership (M4b).
5. Explicit bounds/counter overflow and finite dependency traversal. Pin/release cannot leak handles or silently decrement another host/lease's references. Meaningful valid controls for every adversarial fixture; do not test a guard with an already invalid/consumed handle.

Reuse all existing kernel/resource tests unchanged. New tests may be separate files/modules. Keep implementation in bounded kernel lifecycle/routing code; an additional generic helper module is fine, a new domain schema is not. No external dependencies needed. Document exact correspondence to Workflow.Pipeline and Services.Lifecycle.BoundPipeline/liveReferences and the still-unimplemented workflow/schema/persistence obligations. Root owns fresh review, all12/all3 and commit after focused kernel/resource checks and actual wasm32 builds.

## Remaining M4 composition

M4b adds authoritative module-defined workflow frames/tickets and binds resource plans to operations. M4c adds optional recorded-input and timer services with explicit callback failure/retry semantics and checkpoint ownership. M4d adds real snapshot/codec/restoration, executable/schema validation and supported legacy NetHack integration evidence. These are dependent service boundaries, not exemptions from A4/A5. Full native-to-native collaboration and extension composition must be settled before the M5 migration.

## M4a implementation interface and evidence correspondence

The kernel implements `capture_pipeline` / `invoke_pipeline` /
`pipeline_descriptor` / `release_pipeline`, private ephemeral `PipelineLease`
identities, bounded `retain_owner` / `release_owner`, and
`managed_attach` / `managed_detach` / `managed_unload`. The concrete semantics,
error boundaries, limits and compatibility distinction are documented in the
[kernel README](../../crates/nethacked-kernel/README.md#m4a-retained-execution-and-lifecycle).
Capture conservatively retains every declared dependency of every module in
all resolved chains, including permission-only readonly resource participants.
Cycles refuse; a slot with no active grant at capture retains code but cannot
acquire authority later through that lease. Live grants intersect captured
ceilings and every ancestor ceiling. Inspection and execution share one routing
map. Native artifacts remain host-supplied identities.

Safe boundaries are enforced by exclusive Rust `&mut Kernel` lifecycle access
versus the shared borrow held for the complete invocation, with no mutable
kernel callback interface or guest safe-point claim. Checked instance/serial
identity and reference publication occur only after complete preflight. Legacy
unregister retains tested unpinned behavior but cannot remove pinned or
externally referenced code. Managed unload also checks dependencies and current
extension dispatch. Detach changes future dispatch without invalidating code.

`tests/pinning.rs` supplies populated real Wasm root/provider/nested-extension
old/new execution, multiple leases and external owners, exact identity/removal
protection, live authority changes, readonly ancestors, absent grants, ABI
preflight, committed prefixes, shared nested fuel and memory, traversal limits
and cycles. Private kernel unit fixtures cover otherwise-live counter-overflow
and fabricated identity controls. `resources/tests/pinned_runtime.rs` supplies
real prepare/commit with changed write and permission-only readonly owner
chains, fresh ordinary/new pipeline controls, and still-live pending identities
under revoked/narrowed grants and ancestor scopes. Refusals compare complete
resource/pending/history/counter state before successful restored controls.

This realizes the executable prerequisite of `Workflow.Pipeline` and the host
reference/lifecycle counterpart of `Services.Lifecycle.BoundPipeline` and
`liveReferences`; it makes no cross-language refinement claim. Prepare and
commit have separate retained callable leases, not an authoritative operation
checkpoint. M4b checkpoints/resource-operation ownership, M4c input/timer and
M4d persistence/schema/codec remain pending. Root owns fresh independent review,
full12/all3 acceptance and commit after the focused checks; no acceptance is
claimed by this implementation packet.

## Fuel-fixture interpretation

A nested guest trap/refusal is returned by `kernel.call` as a negative ABI code; a guest may handle or ignore it and retain its already committed prefix. Aggregate-exhaustion fixtures must observe that refusal or explicitly propagate it. They must not assume that dropping the result necessarily traps the root. M4a fuel coverage uses identical 20-call small/heavy workloads, independently successful heavy callees, a measured budget, explicit refusal propagation, and successful controls before/after exhaustion. Runtime budget accounting remains unchanged.

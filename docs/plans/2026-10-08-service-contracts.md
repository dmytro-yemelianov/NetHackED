# Input, timers, persistence and module lifecycle

M2 specifies S4–S7 in [Services.lean](../../NetMechanics/Services.lean), alongside the reviewed [authority](2026-10-08-authority-transaction-contract.md) and [workflow](2026-10-08-workflow-contract.md) contracts. These optional services impose no world, entity, clock, mandatory deterministic rule or production save format on a host. Their separate models have no composed Rust refinement yet.

## Input and RNG

The authenticated caller needs a write grant for the exact provider and named/versioned endpoint. The host also validates ownership of the issued operation/request key. A provider relation accepts a choice and next provider state; the service appends the request, binding and value atomically to its ledger. A seeded stream and externally supplied or nondeterministic input are valid policies.

Replay checks current authority, provider, endpoint, issued key, original request and binding, returns the recorded value, and leaves the entire ledger unchanged. An accepted key cannot be sampled again. If no value is recorded, replay refuses; a real interactive wait uses the workflow protocol. The toy binary provider demonstrably admits two different choices; determinism is an optional property, not a universal assumption.

Vanilla must retain its existing RNG algorithm, state and call order, including conditional calls. The generic model does not prove C draw bounds, PRNG correspondence or a fixed number of primitive draws per request. The runtime must preserve a committed provider/input prefix when a later action cancels or refuses.

## Timers

A task retains its identity, authorized caller/provider endpoint, pipeline, target generations/revisions, profile time/residency and callback payload. Admission checks current authority, a write grant, host, trusted executable binding, exact pinned pipeline, all current target stamps, profile eligibility and a delivery marker. Refusal preserves the queue; successful admission removes matching tasks and marks the identity so another admission cannot deliver it again.

`Policy.ready` selects due, residency and tie-order behavior, with an explicit soundness law. `boundSound` exposes actual callback-code binding as another adapter duty. The task list must have unique identities; this is a stated invariant, not a proved scheduling operation. Rejected/stale-task removal or retry is a separate authorized policy. The once-only theorem concerns admission, not successful callback effects or distributed execution. Runtime callback/transaction coordination remains required.

## Attachment and unload

Attachment checks a host-maintained safe boundary, staged loader validation, unique new artifacts, existing-artifact conflicts and loaded endpoints of dependency edges. It replaces dispatch at one point while preserving existing host state, pending pipelines, timers and owned references. New calls can use the new pipeline; suspended work retains old bindings and code.

Unload refuses an artifact referenced by dispatch, pending work, timers, owned references or a loaded dependent. It also refuses while executing a phase/callback. `ownedReferences` must enumerate real resource/schema/grant references. The abstract dispatch function uses an explicit decidability obligation; a real runtime must provide complete finite reference indexes. Guest claims are not unload evidence.

The loader's remaining check covers ABI/manifests, extension targets, capability conflicts, dependency cycles, compatible schemas, staged initialization and failure containment. That check is trusted here; guest initialization, guest isolation and a concrete executable loader are not implemented. Pending code retention alone does not authorize a schema migration that old work cannot understand.

## Persistence and migration

Restore decodes an envelope, validates its old state before migration, performs an explicitly supported schema migration when needed, then validates the resulting current state before publication. Any refusal leaves the current snapshot unchanged. Same-schema roundtrip follows the declared codec law and successful validations. The identity-wire witness demonstrates migration/refusal only; it is not a JSON/binary save format.

Validators must cover owned module state, executable schemas/bindings, grants, namespaces/counters, pending workflows/transactions, timers and provider/input state. Their soundness and the codec law are adapter obligations. Actual bytes, legacy NetHack migrations, native skill-map ordering, crash recovery and external exactly-once remain runtime work under A5. Restore performs no callback; the runtime must publish validated state before scheduling callbacks.

## Adequacy against retained facts

| Retained fact | Required boundary | Checked model / remaining domain obligation |
|---|---|---|
| Wand direction cancellation can retain charge/time; zero wrest and later exercise have conditional RNG calls | Committed workflow/input prefixes | Ordered workflow and recorded input; exact vanilla draw stream and full zap behavior remain runtime/domain work |
| Whole retirement clears matching references and retains consumed debt | Multi-owner validation and commit | Reviewed authority witness; production owner validators and ports remain to migrate |
| Active/cached objects and callbacks survive level changes | Identity, residency and time are separate | Pinned task/stamp checks and admission replay refusal; actual aging, tie order and handle remapping remain profile work |
| Billing save validation precedes timer catch-up; consumed history differs from live handles | Validate before restore callbacks and supported migration | Restore ordering/refusal; actual NetHack codec and ledger remapping remain tested host adapters |
| Rule changes must attach to a populated host while old pending work survives | New dispatch plus retained executable versions | Attachment/pinning and dependency-aware unload; real guest loading and incompatible-schema refusal remain A2/A4 |

These facts reuse the [architecture evidence table](2026-10-08-microkernel-architecture.md), [wand charge contract](../parity/obligations/2026-10-07-p11a-charge-contract/contract.md), [lifetime facts](../parity/obligations/2026-10-08-p11a-lifetime-contract/README.md) and [billing restoration facts](../parity/obligations/2026-10-07-p11a-billing-save1/README.md). Historical coverage rows are not freshly audited or closed.

The service module has 28 named theorems, including binary-choice/replay, wrong-endpoint refusal, pending/timer preservation, missing dependency, stale timer target/pipeline and supported/unsupported migration witnesses. Authentication, real-code binding, sound validators, non-reused identities, overflow handling and serialized atomic publication remain explicit runtime obligations. The [goal](2026-10-08-microkernel-goal.md) still requires A1–A8.

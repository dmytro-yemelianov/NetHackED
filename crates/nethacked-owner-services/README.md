# Independent owner service composition (M4-O4b)

This optional adapter depends on `nethacked-owners` with `workflows` and
`recorded-inputs`, the real input service and captured-callback timer queue.
Default owners retain no service/game/entity dependency. No World, NetHack
schema, entities, central resource payload database or guest version selector is
used here. `Targets` stores bounded metadata and weak routes; each owner keeps
its own typed allocation. Callback routes are weak; scheduled tasks own bounded
reference bundles, released on cancellation/successful cleanup. Busy cleanup
retains the bundle until retry. Disposal releases the executable pipeline.

`Owner::retain_behavior` captures without running domain queries/preparation.
The opaque reference pairs the original native codec, policy and authenticated
full Wasm helper capture. It is bounded by the host's candidate limit separately
from pending candidates; clones share one reservation. It binds owner identity,
generation/schema, caller/artifact, source operation/pipeline and checkpoint.
Later same-operation phases require monotonic provenance. Each queued callback
owns another bounded scope reservation issued only from its opaque kernel capture.
Source references alone refuse callback execution. Scoped references match the
complete origin (service, capability, slot, endpoint and source) from the live
`CallerView`, then validate
the actual full helper closure against that invocation and current grants.
`retain_from_behavior` authenticates a successor capture before scheduling.
Expired binding routes are weak, bounded and pruned; they retain no historical
codec table. Adapter preflight checks aggregate scope capacity and declared owners
before kernel capture allocates identities. Pins and adapter lookups confer no
authority. Every query/preparation still
requires its own current owner port grant and current dependency stamps; every
publication revalidates exact operation, state/revision/schema and live grants.
Ordinary queries/caches continue requiring current behavior. Retained queries
issue execution-bound observations, permitting first preparation and publication
with old behavior after future admission, without substituting sealed candidates.
Replacement releases the owner's previous codec; only actual live references
retain it. No historical codec table, implicit eviction or identity reuse exists.

The kernel seam is documented before its edit in
[RETENTION-SEAM.md](../nethacked-owners/RETENTION-SEAM.md). It exposes immutable
callback origin only, excluding detached snapshots. Workflows are unchanged.

Inputs use actual `Sample`/`Replay` calls from Wasm. Guest reply IDs are lookup
keys only: `inputs::Service::observation` authenticates the stored accepted record
and exact request, caller/artifact, provider/artifact, permit and legacy operation
scope. Opaque immutable receipts carry policy/version, record, provider, request
and operation identities. Owner ports explicitly declare their bounded input
protocol; codecs receive these immutable observations before pure computation.
Declarations refuse changes while that owner has retained behavior references,
so future setup cannot change the input protocol of captured work.
Use/commit validates receipts without invoking input policy or sampling again.
Accepted records/source changes survive later trap/cancel. Cross-operation replay
remains refused. Recorded query ports force Noncacheable and reject Deterministic
policy; cache validation cannot sample or read live services for query data.
No timer lineage exception is added to legacy input replay scope.

Timer targets retain current incarnation/revision with schema/provider sealed
into weak routes. Scheduling requires the retained behavior's owner port among
the explicit targets. Fixture ceilings are finite key sets. The reused timer
`Clock` profile orders eligible tasks by `(deadline, TaskId)`, requires the
selected residency and current deadline readiness, and honors its explicit
reverse/inactive modes. Event order is declared separately from canonical guard
order. This deterministic fixture permits nondeterministic recorded policies;
it claims no game draw/tie/formula parity.

| Contract/proof-map obligation | Concrete new fixture / retained evidence |
| --- | --- |
| `compatible_future_current`, `prepare_pins_chain`, `EntityFree.old_chain_after_future_replacement` | Owner `composition::baseline_first_preparation_after_future_admission_refuses_without_mutation` proves the baseline gap; `retained_first_query_prepare_and_publish_use_original_codec_and_guest` proves first query/prepare/publication, new algorithm, codec disposal. |
| `wrong_invocation_refused`, bounded identities/references | Owner `retained_behavior_refuses_foreign_owner_operation_and_detached_selection`, `behavior_bound_and_revision_exhaustion_preserve_exact_state_history_and_ids`; adapter `weak_registry_bounds_and_foreign_target_selection_preserve_state_and_release_references`. Existing O2/O4 schema/grant/counter/foreign-caller matrices remain unchanged. |
| `workflow_prefix_composition`, `EntityFree` scalar/device/field | `workflows::paused_recorded_workflow_first_queries_prepares_and_atomically_publishes_old_guests`: independent scalar, device and private field buffer, paused actual workflow, real sample/replay, old algorithms after attachment, new guest algorithm and unload. Helpers import an independent Wasm arithmetic provider; the future extension has its own real import. Native schema query panic guards prohibit a substitute. |
| `derived_input_policy_rejection`, input provenance | `accepted_choice_prefix_survives_trap_and_cancel_without_resampling`; `recorded_authority::recorded_observations_are_credentials_only_when_host_issued_and_freshly_authorized`: forged IDs/requests, detached views, cross-operation replay and input-only grant narrowing at commit. Receipts retain authenticated input identities in commit history. |
| `closed_timer_delivery`, `timer_delivery_refusal`, lifecycle references | `timers::old_captured_callback_first_prepares_after_source_disposal_with_busy_cleanup_and_trap_prefix`: source pipeline disposed before future attachment/first preparation, current grant/readiness/residency refusal, no recursive delivery, committed-prefix trap, Busy cleanup and idempotent owner discard without command replay. |
| Timer successor/retirement/cancellation | `successor_uses_retained_codec_finite_targets_and_profile_ties_without_recursive_delivery`, `stale_retired_targets_cancel_and_dispose_release_behavior_without_execution`, `callback_scope_exhaustion_and_undeclared_owner_refuse_before_capture_ids` (exact queue/kernel counters and history); original timer tie/overflow/profile/refusal matrices reused unchanged. |
| `Equipment.derived_recomputed_result`, `unrelated_cache_stays_valid` | `equipment::equipment_item_equip_retire_and_dynamic_derived_cache_are_coherent_and_local`: independent equipment slot and item contribution, atomic domain equip/retire, guest derived behavior, affected misses/unrelated hits, compatible extension invalidation. This is logical domain retirement, separate from routing tombstoning. |
| `derived_port_closed`, `no_partial_observation` | Owner `cache_and_retained_queries_refuse_at_each_swap_of_a_heterogeneous_batch` adds cache/retained-query probes at both swaps; original O3 publication/drop/allocation instrumentation stays unchanged. |
| Charge/debt/reference accounting and prefix | Reuses unchanged O3 transfer/retirement/accounting and `actual_workflow_phase_trap_cleanup_retry_and_revocation_preserve_prefix`, plus new service prefix/callback composition. No new accounting model or C audit. |

Trust limits: native codec/schema/validator purity, faithful representations,
bounded native work, declared complete targets/participants and no mutable payload
aliases remain trusted host obligations. Runtime callbacks may not bypass owner
visibility. Guest code is fuel/memory/call bounded and pure helper execution
rejects native service reads. Fixture reached counters, positive computed values,
exact prefix/state assertions and explicit expected trap statuses distinguish
refusal from reaching the intended behavior. Rust isolation and these fixtures
are not a Lean refinement or durable/crash atomicity proof. Diagnostic encodings
are not save codecs. Callback participants/retention are trusted host setup;
arbitrary native scheduling-hook side effects are outside the guarantee.

M4d owner restoration, supported migrations, game integration and complete parity
remain open. M4-O4b requires independent integration review; root runs workspace
tests, scoped unchanged goldens and full gates afterward. This writer runs only
affected owner/kernel/workflow/input/timer/adapter tests, formatting, affected
all-target Clippy and actual owner/adapter wasm32 builds.

## Local persistence (D4b)

With `persistence`, typed `Capture::owner` enrollment uses D3 `ExecutionRoots` and
`ExecutionCapture` for exact retained codecs, complete helper closures and callback
scopes. Adapter bytes contain target metadata, ordered route IDs and shared
behavior/scope/bundle IDs. They contain no owner payload copies, trait objects,
pointers or executable credentials. Independently approve the entire adapter
checkpoint against the accepted kernel and owner graph before reconstruction.

Typed hidden reconstruction uses sealed D3 owner metadata enrollment while the
owner visibility gate is closed. Actual weak routes resume ordinary owner checks
after activation. Scope and bundle allocations are shared; the registry retains
weak routes only. Timer reconstruction releases adapter staging roots after every
task owns its references. Drop D3 mapping roots after ownership transfer so cleanup
and disposal can release retained code. Historical target revisions are preserved,
including terminal tasks whose callbacks advanced their owner; stale pending
stamps still refuse at explicit delivery.

Factories/codecs must be independently enrolled, faithful, bounded, pure,
non-panicking, privately allocating and effect-free on drop. Incoming identities
are requests, never approval. Missing old behavior cannot be replaced with current
future dispatch, nor can restoration require a disposed source lease. One-use
claims roll back on failed hidden stages. This seam does not provide the D4e
coherent barrier/epoch activation, D4c historical inputs, D4d resources or A5.

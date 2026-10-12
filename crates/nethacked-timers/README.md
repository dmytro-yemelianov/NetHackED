# Optional timer queue

By default `Service` is an opt-in, ephemeral timer owner; kernel time, entities and RNG are
unchanged. Install its single `queue` major-1 Write port. Guests submit strict
`Request::Schedule`/`Cancel` JSON. Only the issued `TaskId` is returned. The service
checks the authenticated operation, exact provider incarnation, live declared
queue Write permit and invocation scope covering all target addresses. Callback
ceilings are bounded and capture checks that they fit the actual invocation.
The host freezes typed cleanup participants at construction. Guests cannot enroll
participants or supply authoritative caller/artifact/operation identities.

The host supplies a trusted `Profile` and `TargetValidator`. Policies are bounded,
side-effect-free and non-panicking. Profile methods receive detached pending IDs,
deadline/residency bytes and bounded context; `ready` independently confirms the
selection. Profiles define their own clock, residency and tie ordering. The
resource service implements `TargetValidator` by checking the declared stamps
against its detached bounded authoritative state. It confers no resource authority.
Set `validate_on_schedule` to require current targets at publication; delivery
always validates current generations and revisions.

`deliver_next(&mut Kernel, context)` checks readiness, stamps, binding, live
scheduling authority, root request and cleanup bounds before adoption. Refusal
preserves queue, counters, leases and data. Kernel `preflight_callback` shares
validation with `preflight_pipeline`; it does not adopt or recapture dispatch.
The captured old route survives attachment changes and source-operation retirement.
Adoption supplies a fresh operation/lease; the source remains provenance.

Running is published before invocation, then the queue borrow is released. A real
callback can call the same queue to schedule its successor. That new task waits
for a later explicit host delivery. Inspection is allowed during execution;
recursive delivery, installation, cleanup retry and disposal refuse while active.
Authenticated native cancellation can cancel other pending tasks, but cannot
cancel the running task. Cancellation rechecks current queue/target scope and
exact owning caller/artifact; it drops captures and retains a bounded record.
Kernel lifecycle methods require exclusive Kernel access, unavailable while the
invocation borrows it. Retained routes prevent removal of referenced code.
Native ports receive only CallerView, never a dispatcher or whole Kernel/World.

Callbacks receive their payload unchanged and return bounded strict `Outcome`
JSON. Completed and Rejected results are terminal. Trap, invalid/oversized reply
and post-admission errors never make the ID pending again. Committed resource
writes and input records survive every such failure; abandoned plans are discarded.
Cleanup attempts all frozen participants, even after a participant refuses.
Failure retains the lease and terminal record. Only idempotent discard can retry,
through `retry_cleanup`; it never invokes the callback or changes its outcome.
Host participant code must keep any failed cleanup obligations and be idempotent.

Dispose terminal records after cleanup succeeds. Lease release happens before
record removal; a failed release leaves the record retryable. Capacity includes
pending, cancelled and terminal records. Nothing is silently evicted and neither
serial nor queue namespace is reused. Dropping a queue drops pending captures;
adopted leases require explicit disposal (the host owns the Kernel).

## Formal correspondence and limits

In `NetMechanics/Services.lean`, Timer.Task/Key correspond to Task/TaskId plus
stored callback provenance, ceiling and target stamps. Timer.Policy corresponds
to the profile, authenticated queue policy and target validator. Scheduling uses
preflight followed by callback capture/publication. Timer.deliver maps to
current authorization/stamp/readiness checks followed by one-shot adoption and
Running publication. The formal delivered-key ledger is represented by retained
terminal records and non-reused serials after disposal. Pending-only authorized
cancellation is a runtime owner policy; Lean's Timer namespace defines delivery,
not schedule/cancel functions. Runtime terminal outcomes and cleanup
extend the formal admission model using the accepted workflow cleanup convention.

This is neither durable exactly-once delivery nor a complete NetHack timer
adapter. Durable restoration, vanilla clock/RNG/residency policies and domain
migration remain successor work. OOM aborts, native panics and side effects inside
trusted policy callbacks are outside the protocol.

Tests reuse accepted kernel callback and input fixtures and the resource
prepare/commit protocol. Five fixture groups cover policy selection, retained code
and live authority/stamps, real rescheduling/cancellation, committed resource and
input prefixes/cleanup, and atomic admission/capacity/counter/ref-release boundaries.

## Local persistence (D4b)

The optional `persistence` feature adds bounded canonical local checkpoint bytes,
independent capture/restore catalogs and one-use hidden reconstruction. Enroll
`persistence_enrollment()`, `persistence_cleanup()` and `persistence_resources()`
with `kernel_snapshot_with_service_roots`; even empty queues and cancelled
records retain their independently resolved metadata and diagnostic cleanup roots. Bind
capture entries to the actual participant/reference allocations. Shared cleanup
allocations occupy one row; ordered occurrences are retained.

A restore catalog supplies the exact approved profile, target codec/validator,
cleanup factories and optional adapter retention identity. Enroll its staged
`native_port()` with the independent D3 native artifact factory. Whole structural,
catalog and independent context approval precedes old semantic validation.
Rebuild checks the actual kernel destination and current grants before reference
remapping or cleanup construction. No selection, readiness, scheduling, delivery,
callback or discard runs during load. Resume only with explicit `deliver_next`
or terminal `retry_cleanup`. Drop D3 mapping roots after ownership transfer.

Cancelled provenance remains diagnostic history with deliberately invalid kernel
namespace-zero credentials; it cannot authorize execution. Active delivery or a
held queue borrow returns `CaptureBusy`. Failed hidden builds release their claim
and leave the staged port unpublished. Counters, holes and terminal capacity are
preserved, with a fresh queue namespace. Profile/codec fidelity, bounded purity,
stable cleanup identities, fresh private factory allocations, non-panicking work
and effect-free drops are trusted host premises. This is local persistence;
coherent barriers, epochs and atomic session activation remain D4e, and A5 is open.

Timer checkpoint version 2 persists a namespace allocation high-water mark.
Capture includes all queues issued in that process and imported history, even
disposed queues and failed hidden builds. Rebuild atomically reserves that
history and allocates above it and the current process counter; later ordinary
queues also allocate above it. Repeated save/load generations thus refuse every
earlier externally held task ID from that history. Exhaustion returns `Limit`
without wrapping; an exhausted imported history leaves the allocator exhausted.
Failed builds burn allocations, leave the staged port unpublished and permit
claim retry. Version 1 or missing history is refused, since it cannot establish
these guarantees; this uncommitted optional local format does not change native
saves. This covers histories carried by the checkpoint and allocations in the
receiving process, not unrelated independent hosts or later allocations absent
from an older checkpoint.

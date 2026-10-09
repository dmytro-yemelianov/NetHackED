# Optional authoritative workflows

`nethacked-workflows` depends only on the kernel, resource service and existing
serde libraries. It adds no entity, world, RNG, clock, input service or save codec.
The host installs executable modules and supplies a finite `Program` (callable,
authority ceiling, initial phase, phases and allowed edges). `start` validates
bounds and captures the actual pipeline and graph once. Changed code or graphs
apply to future operations. Module state is opaque local scratch; authoritative
resources remain exclusively in `nethacked-resources`.

## Concrete protocol mapping

| Workflow.lean | Runtime |
| --- | --- |
| OperationId.host/serial | kernel-issued private `OperationId`, non-reused checked serial in a kernel namespace |
| Pipeline | retained `PipelineLease` plus immutable `PipelineDescriptor`, including actual routes/code hash |
| Program.allowed/rule | frozen finite `Program.allowed`; real native/Wasm endpoint computes the rule |
| Frame | bounded private owner map; `inspect` returns a detached read-only snapshot |
| Outcome/applyOutcome | bounded serde tagged `Outcome`; state/phase/ordered trace staged until validation and cleanup |
| Step | `step`, running-only, explicit bounded `StepRequest` with sequence/phase/local state/input |
| ticketFor/resume | exact operation, sequence, lease, code hash and phase comparison; resume changes only status/sequence |
| Status | running/suspended/finished/rejected, plus terminal runtime faults |

Requests and outcomes use JSON only for ephemeral endpoint messages, not operation
restoration. `Outcome` tags are `advance`, `suspend`, `finish`, `reject`; unknown
fields/tags refuse. Data fields are byte arrays; phases are bounded strings.
Finish preserves phase; reject preserves phase/local state. Accepted outcomes and
resume each increment sequence once. Events append in module order. All state,
prompt/result, event, trace, graph, request/reply and owner counts are bounded.
The trace reserves the maximum per-step event allowance before invocation; this
can refuse even if a hypothetical outcome would emit fewer events.

## Trust and authenticated ownership

The trusted host controls installation, graph descriptions, initial local state,
ports/grants and cleanup enrollment. Guests control only input interpretation and
bounded outcome data. Guests cannot construct `OperationBinding`: the kernel
creates it from the owner's stored operation/lease/sequence/phase, carries it
through every nested call, and exposes only an immutable value in `CallerView`.
The lease is an exact identity, not just a matching module or hash. Clients must
not use trusted kernel binding/lease-management APIs: those APIs are host
capabilities, never guest exports. JSON operation fields do not authenticate calls.

At start, enroll every phase-scoped service reachable through granted ports in
that operation's bounded `CleanupParticipant` list. Each resource service has a
non-reused private `CleanupIdentity`; the owner freezes identities supplied by
typed participants and includes them in the authenticated binding. Bound prepare
refuses unless its service identity is enrolled, with exact pending/resource
preservation. Thus an omitted participant cannot create an orphan bound plan.
The kernel carries bounded identities, never service instances or a locator.
Resource `Service` implements the typed interface.
Other trusted participants must report their actual service identity (wrappers
forward it) and make discard bounded and idempotent, including
after partial success. The owner retains this frozen participant set.

## Faults, cleanup and lifetime

Admission errors (`Result::Err`) occur before endpoint invocation and preserve
semantic frame/kernel/resource state. Once code is invoked, any invocation error,
trap, malformed/oversized outcome or illegal edge returns an authoritative
`Status::Faulted`: the last local checkpoint remains and committed external effects
remain. The phase never becomes runnable again. There is no whole-phase rollback
and no automatic retry. A guest `kernel.call` refusal is a negative ABI return;
a guest may handle it and return a valid outcome without trapping the root.

Every invoked phase discards its exact epoch's abandoned plans before outcome
publication, including rejected/finished/faulted phases. Committed resource effects
are never discarded. Cleanup failure leaves a terminal operation, original local
checkpoint, saved cleanup binding and all pipeline/participant references. Safe
`retry_cleanup` retries only idempotent discard, never the module; it does not
restore running status. Partial participant cleanup remains safely retryable.
`dispose` refuses pending cleanup or a running/suspended operation and releases
only that operation's lease. Terminal snapshots occupy bounded capacity until
explicit disposal. The host must dispose terminal operations before dropping the
last owner handle; teardown does not use a mutable kernel callback.

Trusted native callbacks/validators/cleanup implementations are bounded and must
not panic. Recoverable allocation reservation refusal precedes code invocation;
process-wide allocator aborts and host panics are outside the execution protocol.
Reentrant inspection/mutation through another owner handle refuses with `Busy`.
No state-mutation API, arbitrary callback on a mutable Kernel/World, or persistence
schema is exposed.

## Evidence fixtures

`tests/runtime.rs` executes real Wasm resource calls for charge/suspend/cancel/
cleanup/time/finish and selection rejection; old/new suspended Wasm operations
retain distinct executable extensions and graphs; exact tickets/replay/reentrancy;
cleanup failure/reference retention/retry; and commit-then-trap/malformed/illegal
no-replay prefixes. Unit fixtures cover sequence overflow, encoding/trace admission,
outcome field bounds, graph/lease admission and failed release preservation.
Kernel `operation_context.rs` checks guest→guest→native propagation; resource
`operation_binding.rs` attacks a still-live plan with other operations, epochs,
phases, leases, plain calls, foreign hosts and forged JSON, then commits with the
original authenticated operation and independently verifies legacy unbound plans.
This establishes generic A4 behavior; populated NetHack migration remains M5.

A trusted host can explicitly `abandon` a running/suspended operation without
invoking a phase or incrementing its counter; it becomes terminal `Abandoned`
while preserving the checkpoint/prefix. This permits safe subsequent disposal
even when a sequence counter is exhausted. Borrow guards prohibit abandonment
during a phase; prior successful phase cleanup already discarded abandoned plans.
A cleanup-faulted operation instead uses cleanup retry before disposal.

## Optional local persistence (D4a)

Enable `persistence` for the workflow-owned `persistence` module. This is a local
prerequisite: it neither replaces a session nor activates a `HostBundle`.
D4e must supply the complete capture barrier, independent context/authority
approval, shared destination epoch exclusion, and atomic whole-bundle exchange.

Enroll exact bindings with `persistence_enrollment`, capture the kernel through
D3 `KernelEnrollment`, then call `capture_local` with its sealed capture index.
`CaptureCatalog` binds approved program/codec identities to actual operations
and participant allocations. All safe boundaries are supported, including
`Running`, suspended and terminal frames with pending cleanup. Held borrows and
in-flight invocation return `Busy`. Capture does not invoke endpoints or cleanup.

The independent `RestoreCatalog` admits exact old programs, state codecs and
participant factories; `FrameAuthority` defaults to refusal and must independently
approve the complete checkpoint, trace and enrollment. Do not approve an incoming
row by echoing it or checking its digest. Validate every local checkpoint against
the D3 snapshot before constructing a kernel or any domain factory. `Validated`
is consumed by `rebuild` using D3's admitted destination map and live grant proof.
A successful reconstruction consumes its kernel workflow enrollment, preventing
replay. A failed hidden build publishes no workflow handles and releases that
reservation. Source state and existing destination objects remain untouched.

Local bytes are bounded canonical JSON; the complete OwnerSave envelope belongs
to D4e. Pure bounded codecs validate approved old semantics before explicit
reference remapping and validate fresh semantics/references afterwards. Opaque
nonreference bytes may be copied; reference-bearing bytes must declare every
module, operation, lease, pipeline-code, cleanup or resource reference. Old local
handles must never be guessed or silently retained. Exact native factory builds,
codec fidelity/purity and effect-free factory/destructor behavior are trusted host
premises. Factories receive only their actual fresh optional cleanup identity;
identityless participants stay enrolled, and shared allocations reconstruct once.
They must not export ungated mutable service handles from a failed build.

Load calls no `start`, `resume`, `step`, endpoint or `discard`. Fresh host tickets
continue the next phase explicitly; old tickets and consumed tickets refuse.
Pending cleanup preserves its exact invocation binding and the accepted ordered
participant prefix. Only explicit `retry_cleanup` on reconstructed workflows continues discard at
the failed participant; it never replays code or a previously accepted discard.
Existing live workflows retain their established idempotent retry behavior. Explicit
terminal disposal releases the exact retained lease once. Kernel counters and
retired identity holes remain kernel-owned.

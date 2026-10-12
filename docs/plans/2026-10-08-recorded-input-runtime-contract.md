# M4c-input: recorded input over authoritative operations

Implement the Input protocol in [Services.lean](../../NetMechanics/Services.lean) using the accepted [workflow runtime](2026-10-08-runtime-workflow-contract.md). This optional service owns its provider source and ordered ledger, not game state. Timers, persistence and the vanilla adapter remain subsequent units; no new world/entity/clock/RNG requirement enters the kernel.

## Concrete boundary

Create `nethacked-inputs` with production dependencies only on the kernel and existing serde/serde_json. Install one versioned native endpoint with write access, accepting a strict tagged `sample {request}` or `replay {id,request}` envelope. Requests and values are bounded opaque bytes. Both actions use the same endpoint and current declared write permission for the service's documented source resource. Deny unknown fields. No operation, ordinal, source replacement, policy or binding supplied in guest JSON is authoritative.

A trusted host supplies an immutable policy identity/version, initial source bytes, limits and a bounded transition callback `(source,request) -> Result<(next_source,value)>`. The callback receives only immutable declared bytes; it must not mutate external authoritative state or perform destructive I/O. It may choose nondeterministically. External events can be represented as an owned source queue established by the host; a staging callback cannot promise rollback of external I/O. Native callback code/identity fidelity, boundedness and absence of panics are explicit host trust. Policy replacement for an existing service is unsupported: use a distinct registered provider/artifact. No compulsory PRNG or deterministic rule.

Require an authenticated `CallerView.operation()`, loaded caller/artifact, the exact installed provider/artifact and current scoped capability permit. Unbound host/guest invocation refuses. Store the operation's private identity, pinned lease, creation checkpoint/phase and caller/artifact/provider/permit with every record. Each service has a non-reused namespace; the service allocates checked record serials and per-operation ordinals. `sample` takes no existing key: every accepted sample is a new choice, even for identical request bytes. Reply includes the service-issued record ID and value. Ordinals reflect actual acceptance order; conditional calls remain conditional.

`replay` never invokes the policy or changes source, counters or ledger. Require exact recorded request, namespace/ID, caller/artifact, provider/artifact and effective capability resolution, plus current permission. Require the same authenticated operation and exact pinned lease. The original creation checkpoint remains immutable provenance. Replay is allowed at that same checkpoint with the same phase, or at a strictly later authoritative checkpoint of the same operation; earlier checkpoints or a different phase at the same checkpoint refuse. This permits replay after a genuine workflow pause/resume without resampling. Current checkpoint context comes from the owner/kernel, never guest metadata. Another operation using the same module, another host, another lease and plain calls cannot consume a record. Original-owner positive controls must remain valid after rejected theft attempts.

## Publication, bounds and lifetime

Stage source and ledger changes together. Before policy execution validate wire/request limits, identity/permission, available record/operation capacity and checked serial/ordinal increments; preflight sufficient reply capacity for the maximum permitted value and ID encoding. Validate callback output sizes and serialize the complete reply before publishing source, counters and appended record in one assignment. Refusal/error preserves exact service state; policy rejection/invalid output may have called the trusted staging callback but publishes nothing. Reentrant service mutation/inspection refuses rather than panics. Finite bounds cover source/request/value/wire bytes, record count and per-operation bookkeeping. No silent eviction, counter reuse or re-sampling of an existing ID. Full capacity refuses; retention/codec extensions can follow without weakening identity rules.

A successfully recorded choice is a committed prefix. Later guest trap, workflow rejection or cancellation does not erase it or restore the prior source. M4b then faults/terminates the phase without automatic retry. Workflow lease ownership already retains executable dependencies while the operation can replay. Historical records alone do not execute callbacks or pin obsolete code; after disposal they are inert receipts. This unit exposes read-only detached inspection, no arbitrary record editing/restoration API and no generic mutable context.

## Required evidence

1. Real guest -> input native port: sequential accepted choices advance the owned source and append records/ordinals in order; replay returns the exact saved value without another policy call. Conditional omitted requests consume nothing; a different valid policy supports externally supplied choices without a seeded stream.
2. A real authoritative workflow samples, suspends, resumes and replays its earlier record under its retained pipeline, then takes the next input. Attach changed executable behavior for a new operation; old replay retains old binding while new operation cannot steal it. Preserve unrelated populated host state. This tests continuation, not durable save/load.
3. Live-record negative matrix with authorized positive controls: other operation/host/caller/artifact/lease, earlier or same-checkpoint wrong phase, unbound invocation, unknown ID/namespace, request mismatch, forged JSON context and current revoked/narrowed grants. Exact source/records/counters comparisons; do not mask cases with already-invalid IDs or grants.
4. Admission bounds and counter overflow refuse before policy execution. Oversized source/value, policy error, reply serialization/size refusal and native reentrancy preserve exact state. Use valid controls proving each rejection condition is reached; minimal unit-only counter setup is permitted.
5. Actual guest samples then traps or returns malformed workflow outcome: recorded input/source prefix survives, workflow becomes terminal, step/resume cannot sample twice. Explicit cancellation also retains accepted choices. Later replay calls never resample accepted IDs.

Keep existing tests/helpers/assertions/goldens byte unchanged. Allowed edits: Cargo.toml/lock, new input crate, this contract only. Reuse existing APIs; report an actual missing boundary rather than change unrelated crates. No new external dependency. Test-only workflow/resources/wat dependencies are allowed for composition fixtures. Run affected input fmt/clippy/tests and actual input wasm32 build; retain commands, exits, source hashes and concise failure history. Root fresh semantic review and full12/all3 precede commit. Document concrete Lean mappings and explicit trust, checkpoint replay and bounded retention policies. No C conformance, vanilla stream migration, codec or family closure claim.

## M4c implementation handoff

The implementation and protocol/trust/Lean mapping documentation are in
[`nethacked-inputs`](../../crates/nethacked-inputs/README.md). Production dependencies
are the existing kernel, serde and serde_json. Tests alone compose workflows,
resources and wat; no kernel, workflow, resource or domain source changes.

All five evidence groups are covered by nine tests: three private unit tests
(checked counters, serializer/reply boundary, live other-provider/host contexts),
four real-guest runtime tests (sequential/conditional/external policy, live negative
matrix, admission/output failures, reentrancy), and two real workflow tests
(retained pipeline continuation with new executable behavior, committed prefixes
under trap/malformed/reject/cancel). Refusal fixtures compare detached complete
service state and callback counts; positive controls use their original live IDs.
Caller/artifact controls share the exact operation and lease. Kernel handles make
same-handle artifact substitution unavailable to guests. Public byte replies have
no data-dependent serialization error; the generic serializer refusal branch is
tested directly with a refusing serializer, not claimed as a reachable guest input.

Commands, raw logs, exits, final source hashes and unrelabelled failure history are
under `/tmp/m4c-input-evidence`. Only focused development/final checks are in scope
here. Root independent semantic review, full12/all3 and commit remain subsequent
acceptance steps. No durable codec, C conformance, vanilla stream migration or
family closure is claimed.

Final focused checks passed: `cargo fmt -p nethacked-inputs -- --check`,
`cargo clippy -p nethacked-inputs --all-targets --locked --offline -- -D warnings`,
`cargo test -p nethacked-inputs --locked --offline` (9 tests), and
`cargo build -p nethacked-inputs --target wasm32-unknown-unknown --locked --offline`.
The two failed development fixture attempts and their single successful repairs
remain in the evidence history. Queue/P1progress edits were not touched.

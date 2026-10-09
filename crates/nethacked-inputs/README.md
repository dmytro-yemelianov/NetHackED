# Recorded input

`Service` is optional host-owned source bytes plus an ordered ledger. It installs
`input` major 1, a native write endpoint. Callers declare that capability and need
current scoped write authority for `{installed_id}/source` for **both** actions:

```json
{"kind":"sample","request":[1,2]}
{"kind":"replay","id":{"namespace":1,"serial":1},"request":[1,2]}
```

The reply is `{"id":{"namespace":...,"serial":...},"value":[...]}`. All objects
deny unknown fields. IDs above illustrate syntax; guests cannot select fresh IDs,
operation identities, ordinals, checkpoints, policies, source or bindings.

Create the service with finite `Limits`, an immutable host `Policy` identity/version,
and initial source. The transition sees only borrowed source/request bytes and
returns staged source/value bytes. A queue, stream or nondeterministic chooser can
implement the policy. Trusted callback identity fidelity, bounded computation and
allocation, no panics, no external authoritative mutation and no destructive I/O
are host obligations. External I/O is not rolled back. Use a distinct service and
registered provider/artifact for another policy; installation is single-use.

Every accepted sample allocates a non-reused service serial and per-operation
ordinal, even for identical request bytes. Admission checks wire/request bytes,
authenticated context, installed incarnation, live scoped permit, capacity,
checked increments and sufficient maximum-value reply capacity before invoking
the callback. Source/value output bounds and complete canonical JSON serialization
precede one state assignment. Errors preserve the exact source, ledger and
counters. Reentrant inspection, installation and port calls refuse through a
fallible borrow. `inspect` returns a detached clone, not a restoration interface.

Replay never calls the policy or publishes state. It requires the original request,
ID/namespace, caller/artifact, provider/artifact, effective capability resolution,
operation and pinned pipeline lease, with a fresh scoped write permit. It accepts
the creation checkpoint only in its creation phase, or a strictly later checkpoint
of that same operation. The host/workflow owns current checkpoint validity; kernel
binding constructors are trusted host APIs, not a guest-controlled clock. Record
creation provenance never changes on replay.

The ledger is a committed prefix. Guest traps, invalid/rejected workflow results
and cancellation retain accepted choices. Workflow-owned leases retain executable
dependencies while replay is possible; records themselves do not acquire kernel
references. Disposal leaves inert receipts. Records and operation bookkeeping have
finite capacity, never evict entries, and refuse when full. There is no timer,
compulsory PRNG or domain-state schema here.

The optional `persistence` feature supplies an input-owned local ledger seam.
Enroll `persistence_operations` with kernel capture, then use
`capture_persistence` with the actual retained observations. Its private checkpoint
preserves source bytes, ordered records, serial/ordinal counters and namespace
history. An independently allowlisted fresh policy factory creates a quarantined
`Restore`; its port refuses calls until `finish` binds every record and observation
to the separately admitted kernel mappings. Loading never calls the transition.
Observation IDs select captured input-owned ledger rows, not owner metadata or
guest credentials. Old namespaces and authority remain invalid. Factories must
be pure, bounded, fresh and free of source aliases or effects on construction/drop.

This local seam does not activate a whole HostBundle or establish an authority
epoch. D4 must compose the service barrier, exact native port factories, kernel
context admission, historical/disposed record orchestration and whole-bundle
activation. D3c owner pending candidate and sealed batch integration remains open;
these local tests do not establish combined persistence acceptance.

`Services.lean/Input`: `Key` maps to authenticated operation plus issued ordinal
(the wire ID additionally has a service namespace/serial); `Record.binding` maps
to recorded caller/provider/artifacts/permit; `Policy.belongs` maps to kernel
operation/lease authentication. Callback execution implements the trusted `rule`
relation. The staged assignment implements `Accept` and `accepted_prefix`;
unseen checked counters implement `accepted_key_cannot_resample`. The replay path
implements `replay_preserves_ledger`, with checkpoint/phase provenance checks in
addition to the Lean binding/request checks. Runtime validation is not a Rust
refinement proof or a determinism claim about arbitrary policies.

Tests use actual Wasm imports and authoritative workflows. The generic serializer
error path is unit-tested with a refusing serializer: public `Reply` consists only
of integers/bytes, so its serialization has no ordinary data-dependent error.
Real endpoint reply-size admission is tested before the policy; the final complete
reply-size check is also exercised at its exact boundary. Counter setup alone is
private/test-only. Distinct caller/artifact checks use nested guests within the
same operation and lease; the kernel prevents replacing an artifact under a live
handle. Cross-host/provider checks use live IDs and valid scoped grants.

Persistence finish rechecks current grants and approved routes before rebuilding
records and observations. Its sealed destination credential binds the actual
service, fresh records, mapped providers/callers/operations and shared pins to the
kernel restore that admitted them. A composing owner restore requires this same
destination; identical checkpoint bytes restored elsewhere cannot attach.

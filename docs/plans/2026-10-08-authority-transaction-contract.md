# Capability authority and atomic transactions

M1 specifies S1/S3 in [Authority.lean](../../NetMechanics/Authority.lean). This is a world-independent protocol model, not a Rust implementation or proof of executable isolation. The [architecture goal](2026-10-08-microkernel-goal.md) remains active.

## Authority boundary

The host authenticates a caller's host namespace, principal and exact loaded artifact. Authoritative bindings, provider exports and grants determine which named/versioned capabilities that caller may read or write. Read grants do not authorize writes. Replacing code or revoking a grant prevents the old prepared operation from committing.

Artifact identifiers stand for a trusted binding to real executable code, version and configuration. The model does not prove cryptography, guest isolation, or authentication of self-attested caller fields. `RuntimeTrust` names those adapter obligations; registry membership alone is not an authentication proof. The runtime must construct invocation identities from trusted execution context.

The resource protocol is a service with arbitrary payload and event types. Its addresses and owners are not mandatory gameplay entities. The eventual kernel may provide it to modules without imposing it on every host. `Authority.Capability` and `Architecture.Capability` still need an explicit runtime correspondence; their similar records are not a verified bridge.

## Prepare and commit

1. A plan declares readonly dependencies, writes and ordered events. Every participant has an owner, resource key, generation, revision and provider capability.
2. Prepare validates caller authority, current stamps, provider bindings, read/write grants, unique participants, a participant bound, owner checks and accounting checks. Owners see only declared snapshots. Refusal returns the original complete state. Success stores the exact plan and caller in authoritative pending storage, increments the serial and leaves resources/history untouched.
3. Commit accepts only the transaction identity. It resolves the stored plan and rechecks the original caller, current code/grants, resource generations/revisions, owner checks and accounting. Callers cannot replace its writes or event list.
4. Success patches all declared writes together, clears the pending entry and appends its events once. Refusal preserves all state, including pending storage, history and counters. Resources outside the write set remain unchanged. Replaying the committed identity fails against the resulting authoritative state.

The host must serialize revalidation, mutation and publication as one atomic operation. The pure transition proves no properties of concurrent Rust, callback reentrancy or external side effects. Those require runtime enforcement. A refusal here preserves this transaction's starting state; it does not roll back an earlier committed workflow prefix.

## Checked statements and retained facts

The module has 48 named theorems: general validity/refusal/write/frame/history/replay/allocation laws and 23 executable witnesses. Witnesses cover wrong host/code, read-only and wrong-version grants, removed providers, revocation/replacement, reused generations, changed write or readonly revisions, invalid accounting/owner checks, duplicates and participant limits.

The [retained wand lifetime facts](../parity/obligations/2026-10-08-p11a-lifetime-contract/README.md) motivate a multi-owner witness: whole item deletion, matching equipment/timer cleanup, and retained consumed shop debt with quantity, terms, creditor and liability preserved. An unrelated resource and existing history survive. Invalid accounting refuses before cleanup. These are concrete examples of the architecture contract, not a complete wand implementation or fresh C equivalence audit.

## Limits and runtime acceptance

- Host/serial identities, generations and artifact bindings must not be reused. The allocation invariant is proved through this model's prepare/commit transitions; restoring or supplying arbitrary state must validate it independently.
- Counters use unbounded `Nat`. Runtime overflow must refuse before changes, or use an equivalent representation that does not reuse identities.
- Owner/accounting predicates are host-supplied, with soundness laws assumed in `Policy`. The model cannot establish the correctness of arbitrary validators.
- A pending plan must remain authoritative and immutable. Rolling back the entire session snapshot can replay old effects unless the host supplies an additional durable policy; distributed exactly-once is not claimed.
- The workflow frame and transaction state are separate specifications. The runtime must bind a transaction to its authenticated workflow/pipeline; no composed refinement theorem or concrete codec exists yet.
- General resource creation, schema migration, entropy/input streams, timer ownership and attachment/unload are later contracts. This unit does not resolve them implicitly.

Acceptance requires the goal's A3 scenarios through real runtime ports, followed by the A6 NetHack vertical slice. A passing Lean build alone closes neither scenario.

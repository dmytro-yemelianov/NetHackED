# Ordered workflows and continuation ownership

[Workflow.lean](../../NetMechanics/Workflow.lean) extends the [first architecture model](2026-10-08-microkernel-architecture.md). It specifies module-owned execution without a required world, entity, clock or deterministic rule.

## Protocol

A module supplies a `Program`: its pipeline identity, phase graph and relational rule. Phases and allowed transitions are domain types/policy; the kernel does not impose a common sequence of game actions. Extensions can change the program for future operations. An existing operation retains its pinned program until completion or explicit migration.

Each `Frame` records the host/operation identity, sequence number, pinned pipeline, module-local state, phase, committed event trace and status. The pipeline includes owner/provider manifests, ordered extensions and an opaque code identity. The runtime must bind that identity to actual code and capability implementations.

There are four step outcomes:

| Outcome | Meaning |
|---|---|
| `advance` | Commit local state/events and follow a declared edge |
| `suspend` | Commit local state/events, retain the next phase and request a reply |
| `finish` | Commit local state/events and return the module's result type |
| `reject` | Preserve the latest checkpoint's local state, append optional diagnostic events and terminate |

Rejecting a step does **not** roll back an earlier committed prefix. A suspended frame cannot execute another step until resumed; a finished frame cannot execute more steps. Event batches append in execution order. `ScopedStep` connects the workflow to the existing `Scope` adapter and preserves its declared unrelated-state frame.

## Resume and persistence

A `Ticket` contains host/operation identity, sequence, pipeline identity and resume phase. `resume` checks the authoritative current frame: it must be suspended, the ticket must match, and the selected pipeline must equal the captured pipeline. Successful resume preserves local state, phase and trace, changes status to running and increments the sequence. An older sequence is rejected even if the operation suspends again. Another host or operation's ticket is rejected.

Attaching an extension can affect new operations while a paused operation uses its retained old pipeline. Passing a replacement owner, provider, extension list or code identity as that old continuation's pipeline fails. This is not a ban on attaching modules to a live session; it requires retained code/bindings or explicit continuation migration.

`CheckpointCodec` states a required encode/decode roundtrip law. Under that law, restoring a checkpoint preserves its phase and the result of `resume`. There is no verified concrete serializer, schema upgrade or corrupt-save validator here. The law does not imply identical future random choices or external input: those remain profile contracts.

The model requires the caller to retain one authoritative current frame. It does not prove ID allocation, token secrecy, atomic publication, distributed exactly-once behavior, resistance to restoring an older whole session, or safe resource borrowing across a pause. Runtime tickets must refer to stored checkpoints, rather than carry arbitrary state supplied by a client. An actual step must revalidate resource handles/versions through granted ports; borrowed host references are not serialized.

## NetHack fact witness

The accepted [wand source contract](../parity/obligations/2026-10-07-p11a-charge-contract/contract.md) distinguishes cancelling selection before charge from cancelling direction after charge. The latter retains the spent charge and action time and reaches cleanup; its C command result is `ECMD_TIME`, not `ECMD_CANCEL`.

The checked example deliberately covers the non-backfiring positive-charge branch. It starts with one charge, suspends after decrement, resumes at direction, processes direction cancellation, visits retirement and spends action time. `direction_cancel_retains_charge_and_time` proves zero remaining charges, exactly one charge write, one action-time charge, the ordered abstract event trace, and a finished `time` result. `timeCharges` is an action-cost counter, not a universal clock or proof of a scheduler-turn increment. Event names are abstract labels, not a transcription of C message text.

Examples reject replay after resume, replay after a new suspension, a different host/operation, and changed owner version, provider set, extension set or code identity. Other signs/wrest branches, backfire, actual effects, blindness, naming, equipment/bill cleanup and frontend wire formats remain outside this witness.

## Remaining architecture work

This completes the first workflow/continuation contract, with an assumed persistence interface. Capability/code enforcement, cross-owner transaction implementation, RNG/input trace policy, timer ownership, actual persistence/migrations and attachment/unloading lifecycle still need their own formal contracts and runtime implementations. No Rust migration or full parity acceptance follows from this module.

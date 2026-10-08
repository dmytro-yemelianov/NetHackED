# M4b: authoritative workflows and operation-owned transactions

Implement [Workflow.lean](../../NetMechanics/Workflow.lean)'s execution/resume protocol on the accepted [retained pipelines](2026-10-08-pinned-execution-contract.md) and [resource service](2026-10-08-resource-service-contract.md). M4c input/timers and M4d codec/restoration remain dependent work. This child must establish actual A4 pause/extension/resume behavior, not just a copyable frame struct.

## Authoritative owner and module program

Create optional `nethacked-workflows`, independent of every NetHack domain crate. It owns a bounded authoritative map of operation frames. An operation retains an immutable module-defined program (initial phase, finite permitted transitions and callable), its actual captured executable pipeline, opaque module-local state, phase, sequence, ordered trace, and running/suspended/finished/rejected/faulted status. Phase names/edges, prompts, results, state and events are module/profile data, never a fixed game command or mandatory world/entity/clock/RNG schema. A trusted host supplies a bounded program descriptor bound to the callable captured at start; each frame freezes that descriptor and actual resolution. Future operations can receive changed descriptors/code. No unchecked mutable frame or universal host context is exposed to clients/modules.

Invoke the captured native/Wasm module with a bounded explicit step-request payload containing the authoritative local checkpoint, phase and external input. Decode a bounded tagged outcome: advance(next,state,events), suspend(next,prompt,state,events), finish(result,state,events), reject(events). Validate allowed phase edges and all lengths/counters before publishing outcome state. Reject preserves the prior module-local state/phase and appends its diagnostics; every accepted outcome advances sequence once. The module can use granted ports for owned resources; workflow-local scratch must not become duplicate authoritative world/resource storage. An input can be externally chosen; no universal deterministic-rule assumption.

A suspended or terminal frame cannot step. Resume accepts a ticket for the exact stored operation namespace/serial, sequence, pinned pipeline identity and phase. Compare all fields to the authoritative suspended frame; wrong host/operation/sequence/phase/code refuses without any state change. A successful resume preserves phase/local state/trace and advances sequence once while making the frame running. Client-provided state, graph or replacement pipeline cannot become authoritative. Reusing a ticket after another suspension must fail. Ticket transport encoding is not a save codec or permission to forge invocation context.

## Failure and committed prefixes

Pre-execution refusal (unknown ID, invalid status/ticket, input/size/counter/capacity/serialization bounds) leaves all workflow/kernel metadata and resources unchanged. Preflight bounded request encoding, event/trace capacity and counter increments before invoking guest code. Reserve enough metadata capacity to record a terminal failure without a second fallible publication.

Once a phase was actually invoked, native/resource effects may already form a committed prefix. Guest trap, malformed/oversized outcome or illegal transition must produce an authoritative terminal fault, preserving the last local checkpoint and all earlier committed effects; it must never leave the frame apparently runnable or silently retry that phase. Do not promise whole-phase rollback across arbitrary native effects. Local-state/trace publication is staged and atomic; multi-owner resource mutation remains the separate accepted atomic transaction protocol. Native callbacks/validators remain trusted and bounded. Reentrant workflow mutation refuses. Explicit reject/finish is terminal. Admission failures and post-invocation faults are distinct tested results.

## Authenticated operation ownership

Add the smallest kernel mechanism needed to carry an immutable operation/checkpoint binding through the entire retained invocation and all nested native/guest calls. Issue host-bound non-reused operation identity through a trusted host API; guest payloads cannot select or forge it. The binding includes exact retained pipeline and checkpoint/phase epoch, is constructed from the runtime's stored frame, and appears in CallerView. Domain callbacks still receive no mutable Kernel/World. Plain legacy invocations have no operation binding.

Extend prepared resource plans to capture and recheck this binding in addition to existing caller/artifact/provider/permit checks. A second operation using the same module must not commit the first operation's plan. Different host, checkpoint epoch, pipeline, standalone/unbound invocation and forged JSON bindings must refuse with exact resource/pending/history preservation. Existing unbound M3 resource behavior remains available and cannot adopt a bound plan. The ordinary guest commit payload remains only the transaction ID.

For M4b, prepared plans are scoped to one phase/checkpoint epoch: commit within that phase; advancing/suspending/resuming/terminating must not silently promote a stale plan to a new epoch. Provide an explicit bounded owner cleanup path for abandoned operation plans and integrate it into the workflow lifecycle (or retain the operation and its code references until cleanup succeeds). No unbounded orphan plans or released code still required by live pending work. Use explicit typed participant/cleanup interfaces for optional services; no universal service locator or mandatory resource state in the kernel. Cleanup errors must leave a terminal, non-replayable operation with required references retained and an explicit safe retry path. Resources are changed only through the resource service; its cleanup may discard owned pending plans, not roll back committed effects.

Workflow start/retirement must own/release the corresponding retained pipeline/reference set, enforce limits and checked counters, and avoid leaked references on any failure. Terminal inspection may remain available boundedly; explicit disposal cannot invalidate another operation's shared code references. Runtime-generated binding metadata is not a guest assertion, and adapters must never rely on a merely supplied operation ID as proof of current execution.

## Required evidence

1. An actual Wasm workflow on a populated entity-free host charges a resource once, suspends for a direction-like reply, then cancellation resumes through ordered cleanup/time phases and finishes with charge/time each once. Selection-like early rejection has no resource effect. The fixture uses generic data, not a new NetHack wand implementation or full C parity claim.
2. Suspend an old operation, attach an executable extension with changed behavior, start a new operation, and resume old/new under their respective frozen graphs/resolved code. Verify unrelated state, reference protection and safe disposal. This completes generic A4; populated NetHack integration is still M5.
3. Wrong host/operation/ticket sequence/phase/code, replay after a later suspension, stepping suspended/terminal, illegal edges, bounds and counter overflow have otherwise valid controls and exact state comparisons.
4. Distinct operations of the same loaded module, changed checkpoint epoch, plain invocation and forged client metadata cannot steal a live prepared plan; the original authorized same-phase operation succeeds. Abandoned plans are cleaned safely with references retained through failure/retry. Exercise real guest->resource ports, not only constructed CallerView unit values.
5. Trap or malformed outcome after an actual resource commit preserves that effect and faults the operation; another step/resume cannot duplicate it. Pre-invocation refusal preserves everything. Test native callback reentrancy and allocation/release failure paths as applicable.

Use existing kernel/resources/serde/serde_json; wat is already available for tests. No new external dependencies. Existing tests/helpers/assertions/golds unchanged; new tests may be separate files. Document concrete Lean-to-runtime mappings and explicit trust/fault/cleanup policies. Focused affected-package tests/clippy/fmt and actual workflow/kernel/resource wasm32 builds precede root's fresh semantic review, full12/all3 and commit. No persistence/schema codec, input/timer implementation, domain migration or unrelated process changes in this child.

## M4b implementation mapping and review scope

The optional [workflow crate](../../crates/nethacked-workflows/README.md) implements
this protocol with frozen finite `Program`, private bounded frames, real retained
native/Wasm invocation, tagged byte-array outcomes, exact tickets and terminal
runtime faults. The README defines concrete Lean mappings, conservative trace
admission, explicit host trust/wiring, JSON endpoint transport, cleanup retry and
explicit lease disposal. The kernel authenticates nested invocation context; the
resource service captures/rechecks that context and discards exact epoch plans.
Host-enrolled typed participants are the explicit ownership contract for services
reachable through grants, not an inferred service locator. No guest-selected
identity, durable codec, domain resource copy, clock or RNG was added.

Focused fixture groups cover Wasm charge/cancel/time once; both old/new paused
Wasm code and graphs; exact tickets/bounds/overflow/reentrancy; real port live-plan
theft with authorized and legacy controls plus cleanup failure/retry; and committed
prefix traps/invalid outcomes without replay. Root's fresh semantic review, full
workspace gates/goldens and commit remain subsequent work; this child does not
modify the queue/P1 progress or existing tests/helpers/assertions.

Cleanup enrollment is enforced at bound resource preparation: the immutable
operation binding contains the bounded identities of its frozen typed participant
set. A service missing from that set refuses before preparing anything. Identities
are non-reused host tokens, carry no service lookup, and are never guest assertions.

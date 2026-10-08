# Microkernel architecture: specifications and execution plan

Status: active goal, created on 2026-10-08. This plan governs architecture work. The [53-family parity backlog](2026-10-07-remaining-parity-scope.md) is retained separately: migrating existing behavior does not implement or close missing NetHack branches.

## Goal and completion boundary

Build and integrate a kernel → modules → extensions architecture in which executable extensions attach to existing sessions and alter rules. A world is an optional host, not a kernel schema. Complete the formal contracts, implement the runtime, migrate the existing NetHack domain logic to bounded capabilities, preserve compatibility, and verify two materially different hosts. Documentation or a standalone prototype alone does not complete this goal.

The initial [Lean architecture](../../NetMechanics/Architecture.lean) and [workflow protocol](../../NetMechanics/Workflow.lean) have passed their first-model reviews. They are formal specifications; they do not establish Rust correspondence or a working loader. Their detailed scope is in the [architecture description](2026-10-08-microkernel-architecture.md) and [workflow contract](2026-10-08-workflow-contract.md).

## Required specifications

| ID | Contract | Required observable behavior |
|---|---|---|
| S1 | Capability authority and code binding | A module/extension receives named, versioned operation grants bound to the loaded artifact and host. An unauthorized read, mutation or call is refused before state changes. No universal host/context escape hatch |
| S2 | Ordered workflows and outcomes | Module-defined phase graphs and relational rules; committed prefixes survive pauses and later refusals; cancellation costs are profile behavior; effects are ordered; terminal operations stop |
| S3 | Cross-owner atomic mutation | Preflight validates grants, generations, resource versions and accounting before cleanup. A ticket commits once against current authoritative state. Conflict/refusal preserves all owners; success updates the declared participants together |
| S4 | RNG and external input | Entropy/input providers are explicit capabilities. Vanilla preserves its existing stream and draw order; other profiles can use other policies. Replay records inputs/policies rather than pretending every module is deterministic |
| S5 | Timers and continuation ownership | Callbacks and pending operations retain owner, identity and pipeline bindings. Stale replies/targets are refused. Time units, tie ordering and active/off-level residency are profile services, not mandatory kernel fields |
| S6 | Persistence and migrations | Persist module schemas, owned state, active pipelines, timers and RNG/input state as applicable. Validate before restoration/callbacks. A real codec and explicit migration preserve declared continuation semantics and supported old saves |
| S7 | Attachment, replacement and unload | Attach at a safe operation boundary without reconstructing the host. New operations can use new rules while pending operations retain their bindings. Missing dependencies, conflicts and unsafe unload/replacement are refused atomically |

Formal definitions may expose explicit adapter/codec/code-binding assumptions. Such assumptions must become tested runtime obligations; an assumed law is not implementation acceptance.

## Runtime boundaries

- The new generic kernel has no dependency on NetHack domain crates, no compulsory actor/item arena, map, hero, clock or game action enum. It owns registration, capability enforcement and operation metadata.
- Modules own domain state and algorithms. Read-only context and mutable views are separate. Shared resources have a single authoritative owner; dependent modules use typed ports rather than duplicate state.
- Extensions target exported points or contribute new modules/commands under their namespace. They can alter algorithms, not only constants. An executable backend must load without rebuilding the engine; native and browser compatibility are selection criteria.
- Kernel structural guarantees enclose extension behavior. A guest cannot bypass authority checks, overwrite operation identity or replace a pending pipeline. Resource budgets and failure containment must be specified by the chosen backend.
- `SimulationWorld` may remain a compatibility facade and NetHack composition owner. Production domain modules must stop taking the entire `SimulationWorld`, importing it as their state API, or depending on a renamed universal context.
- Existing pure mechanics and usable data/storage implementations are retained behind the new boundaries. This is an extraction/refactoring migration, not a blanket replacement of tested formulas.

## Acceptance scenarios

| ID | Scenario | Pass condition |
|---|---|---|
| A1 | Entity-free host | A populated scalar/field/device session runs through the kernel without NetHack, map, hero or entity dependencies; this is a software host example, not a physics claim |
| A2 | Live executable extension | Load an extension into an existing populated host, invoke its added/changed behavior, and preserve unrelated state; no engine rebuild or session reconstruction |
| A3 | Authority/conflict boundary | Unauthorized operations, wrong host/generation, stale resource versions and duplicate commit are refused without partial mutation; unrelated owners and event history survive |
| A4 | Pending-operation replacement | Pause under one pipeline, attach a new extension, continue the old operation under retained bindings, and start a new operation under the new pipeline; unsupported replacement/migration is refused |
| A5 | Concrete persistence | Save/load a paused operation and module-owned state, resume without duplicate effects/charge, verify applicable RNG/input continuation, and exercise supported legacy NetHack saves |
| A6 | NetHack vertical slice | Drive a complete selected-wand workflow through native public commands, item lifetime, equipment/timer references and shop debt under module ports. Preserve existing identity/accounting invariants and source-derived ordering |
| A7 | Existing-domain migration | Every current production NetHack subsystem is assigned to a module owner; direct universal-world access is confined to the facade/host adapters and frontend compatibility surfaces, with a checked dependency inventory |
| A8 | Final integration | Fresh independent reviews, all required compatibility checks, the full twelve gates and all three scoped unchanged goldens pass on committed inputs. No unresolved in-scope architecture blocker |

## Dependency-ordered milestones

| Milestone | Work and prerequisites | Exit |
|---|---|---|
| M0 | Specs/plan, worktree hygiene and migration inventory | Goal/queue recorded; preserve the interrupted wand draft as a hash-bound archive before restoring its agent-owned unaccepted changes for clean architecture commits; enumerate current universal-world dependencies |
| M1 | Complete S1/S3 against retained item-retirement and debt facts | Checked Lean authority/versioned prepare/commit model; refusal, replay and unrelated-state proofs; source-fact witnesses |
| M2 | Complete S4–S7 using S1–S3 and existing workflow proofs | Checked RNG/input, timer, persistence and lifecycle specifications; concrete counterexamples/assumptions tracked; architecture adequacy matrix reconciled |
| M3 | Generic runtime and extension ABI/backend selection after M1/M2 | Kernel authority/state/operation interfaces implemented; executable backend justified against requirements; A1/A2/A3 pass |
| M4 | Runtime workflow, snapshot/codec and lifecycle services after M3 | A4/A5 pass with real storage/code binding; proof-to-test mappings and error policies documented |
| M5 | NetHack host adapters and complete wand vertical slice after M4 | A6 passes; facade/native-save/API compatibility retained; no whole-world parameter in the extracted modules |
| M6 | Migrate remaining existing domain groups through the established interfaces | A7 passes: equipment/properties, combat, actors/AI, remaining objects/tools, hero/progression, spatial/content, economy/NPCs, commands/presentation and persistence/endings |
| M7 | Integration and completion audit after every migration group | A8 passes; clean reviewed commits; remaining behavior-parity work explicitly stays in its original backlog |

The reviewed [autonomous ownership contract](2026-10-08-autonomous-ownership-contract.md) strengthens M4 before persistence: O1 formal ownership → O2 independent owner storage/commands → O3 atomic publication → O4 live behavior/service integration. M4d depends on the accepted O2–O4 runtime, not just the contract or the centralized resource prototype. Owners maintain private state and behavior; routing stores metadata only. M5–M7 and the separate behavior-parity backlog retain their full scope.

O4 executes in two dependent implementation slices to keep source and review packets bounded. O4a adds compatible live Wasm behavior admission and authenticated, versioned derived queries/caches over independent owners. O4b composes those APIs with actual recorded inputs, retained timers and workflows, including scalar/device/field and equipment/lifecycle witnesses. O4 is accepted only after both slices and a fresh integration review; M4d still depends on that aggregate acceptance. Existing O2/O3 semantics and fixtures remain prerequisites, not substitutes for the new witnesses.

O4b and the aggregate generic owner runtime are now accepted in the [service integration checkpoint](../parity/obligations/2026-10-08-microkernel-owner-services-checkpoint.json). Real retained workflows and timer callbacks first query/prepare after future behavior admission, recorded inputs are authenticated before pure preparation, and metadata-only routes compose independent scalar/device/field and equipment owners. Independent review, refusal/mutation controls and full gates cover this boundary. M4d concrete codecs/restoration is next; NetHack integration and complete domain migration remain M5–M7.

Each milestone is split only at testable ownership boundaries. Dependency edges are enforced before dispatch. A data-only patch system or an extension compiled into the original engine does not satisfy A2. Forwarding `&mut SimulationWorld` through a new module does not satisfy A7.

## Execution loop and cost controls

Apply the accepted [execution process](2026-10-07-parity-process-optimization.md): one writer and one active child, no nested fleet, compact hash-bound packets, bounded source/log excerpts and local mechanical checks. Reuse known facts and accepted interfaces; do not run a new source-contract agent for every migrated function.

Use Sol/medium for ordinary implementation, Sol/high for demonstrated lifecycle/RNG/order complexity and fresh semantic reviews; Luna is for narrow missing facts or a planning-only check. Root model/effort and unavailable usage remain unknown. Prior architecture reviews used 26,945 and 30,480 uncached input tokens; both crossed their respective soft thresholds. This is not evidence of achieved savings. Subsequent packets must contain only the affected contract and factual witnesses, with no architecture-history or complete-registry reload.

Develop with affected checks, then perform one final focused allocation and a fresh review. Run the full gates and goldens before code commits. Group compatible formal foundations into meaningful checkpoints instead of creating a separate implementation/contract/review loop for every small theorem. Retain concrete failure/retry history; no resetting counters or silent scope exclusions.

Update P1 progress and the architecture queue before/after each unit. Native goal remains active until A1–A8 and the final audit are satisfied. Elapsed time, a passing Lean build, a prototype, or an exhausted soft threshold does not complete it. No new golden rebaseline is granted by this goal.

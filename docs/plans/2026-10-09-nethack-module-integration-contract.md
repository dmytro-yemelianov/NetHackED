# M5 NetHack module integration contract

Baseline: `1b1e6f5`; authorized branch: `feat/monster-hp-d8`. This specifies implementation, not completion evidence. Accepted D1–D6 and the A5 checkpoint remain prerequisites without predecessor re-audit. Root owns source evidence, review, full gates and golden determinism verification. No new dependencies, golden changes or claim of closing all 53 parity families.

## Production composition and authority

M5 delivers the goal plan's complete A6 selected-wand slice and aggregate A2/A4 acceptance. Extend `owner_persistence/runtime.rs`'s `SourceRoutes`, `native_ports` and `OwnerBridge` composition into an independently populated NetHack owner host. Its public command dispatcher routes selected-wand commands to an actual wand caller module, proposed as `actions/wand.rs`. That module executes the shared wand algorithm and declares its collaborators; billing must not remain the surrogate gameplay caller.

`partitions.rs` state types remain owner-local storage. Item authority owns identities, definitions, quantities, charges and placement; equipment owns slot/held references; clock owns time, timer registrations and tie/residency rules; billing owns debt, terms and consumed receipts; random owns draws and stream position; geography owns cells, residency and trajectory observations; actors own actor identity, position, health and effects. Hero/session authorities retain their existing state responsibilities. No collaborator can mutate another owner's storage.

The production host initializes/imports once and thereafter executes real public commands against these owners. `SimulationWorld` remains an isolated compatibility host: `actions/items.rs::handle_zap_selected_wand` and `handle_zap_item` delegate through narrow borrowed item/equipment/clock/billing/random/geography/actor adapters to the same algorithms. Shared wand/domain algorithms import neither `SimulationWorld` nor a universal context. Gameplay never exports/reconstructs a World, keeps a mutable World mirror, or serializes entire partitions into requests. Detached World conversion remains only the accepted native-save import/export boundary.

## Command and collaboration protocol

1. Public input supplies inventory index, optional expected `ItemId`, direction and energy. Item authority resolves the ordered carried selection; an absent/mismatched identity or non-wand preserves the existing refusal/event behavior. Resolve canonical/custom definition identity through its approved ruleset binding; unavailable explicit definitions cannot fall back to display names.
2. Capture the operation's executable chain and declared participant routes. Observe only selected item identity/generation, revision, class, quantity, charges, BUC, definition and placement; bounded hero polymorph state and session locale/ruleset profile through declared typed ports; relevant equipment references; matching timers; selected billing entries/receipt admission; actor origin/targets; bounded trajectory cells; and random reservation metadata. Large trajectories use finite versioned segments. Observations carry owner/schema/revision/binding provenance, never whole-owner snapshots or mutable aliases.
3. Wand logic requests typed proposals for charge use, effects, movement, references, timers, debt and random draws in the accepted source-derived order. Dependencies are declared before preparation. Native-to-native calls are through granted typed ports: no recursive public-command invocation, hidden lookup, generic setter or unlimited fanout. Each command declares finite target/cell/draw bounds; exhaustion refuses before that publication. Continuations resume through the workflow owner rather than an unbounded call stack.
4. Use the existing sealed preparation/publication protocol. Revalidate live authority, generations, relevant revisions, accounting and bindings before exchange. A sealed ticket publishes once across its declared participants. A stale observation, revoked grant or duplicate ticket produces no additional charge, draw, timer, receipt or event. Never introduce a second transaction protocol.
5. Deliver events and advance command/timer work in existing order. Earlier successfully published prefixes survive a later refusal or pause; only unpublished proposals are discarded. Random advances only for accepted draws at their established publication points. Save/restore preserves those prefixes, outstanding operations and consumed tickets without replaying effects.

## Identity, lifetime and accounting

Item identity is arena identity plus generation, independent of name, definition identity and executable version. Quantity belongs to that identity; quantity-one and quantity-three fixtures distinguish partial-stack survival from whole-item retirement. Split/merge work, when represented, must explicitly preserve or retire identities and transfer quantities/references/accounting without inventing semantics for unimplemented C behavior.

The accepted finalizer retires only an explicitly surviving same-generation wand with negative charges. Cleared, stale, retired and nonnegative targets do nothing. Before dust/event publication it validates billing and every represented reference, clears only matching equipment/timer references and commits whole disposal together. Actor/geography placement and cached/off-level references cannot dangle. Consumed billing evidence survives physical retirement with identity, quantity, terms and accepted receipt order; unknown debt remains unknown, never silently zero or recomputed. D5 `record_bill_terms` receipt validation, single admission and duplicate refusal remain authoritative.

Restoration preserves legacy absence/tag/artifact/equipment/timer migrations, canonical/custom definitions, old pack/hash/parser approval and native-save behavior. Old code and codecs require their retained artifact/ABI/schema bindings and current approval/grants. Validate old bytes before invoking approved migration; unavailable or incompatible code/codecs refuse before destination exchange. Restore timers only after complete admission under accepted catchup/reinsertion ordering. No automatic owner-restoration scheduling or authority resurrection.

## Dependency-ordered implementation units

Every unit supplies finite positive/refusal fixtures; root reviews evidence and runs required gates before acceptance.

| Unit; prerequisites | Implementation boundary | Positive / refusal acceptance |
|---|---|---|
| I1; D1–D6/A5 | Typed observations, proposals and owner registrations in runtime/partitions; separate authorities and bounds | Selected-item/cell/reference queries preserve unrelated populated state / wrong host, generation, scope, schema and exceeded bounds preserve state and effect counters. |
| I2; I1 | Actual wand caller and shared algorithm; borrowed legacy adapters; both public dispatchers | Indexed/expected-identity zaps produce matching charges, effects, RNG and events on both hosts, including polymorphed/unpolymorphed creation and localized empty-wand events / missing selection, mismatch, non-wand, unavailable explicit definition; structural check forbids World/universal-context imports and per-call reconstruction. |
| I3; I2 | Sealed cross-owner lifetime/equipment/timer/shop publications | Cross quantity 1/3, billed/unbilled, known/unknown debt, active/cached placement; matching-only cleanup, tied/deferred timers and receipt persistence / stale reuse, cleared target, nonnegative charges, duplicate publication and conflicting accounting; no extra effects after committed prefix. |
| I4; I3 | Production extension attachment, continuation capture and fresh-process persistence through accepted registries | Load executable extension into populated NetHack host changing wand algorithm; pause old operation, attach new code, save, restore, resume old retained code, then execute new command using new code; unrelated state survives / missing retained artifact/codec, revoked approval, incompatible schema and unsupported migration preserve populated destination. |
| I5; I4 | Aggregate production acceptance and compatibility fixtures | One public-command suite combines A2 executable attachment, A4 retained-code continuation across restore, and A6 full selection→effect→lifetime→equipment/timer→shop→native save/reload workflow / all preceding refusals plus stale replies and repeated receipt admission, with unchanged unrelated owners and ordered event/draw prefixes. |

## Acceptance boundary

I5 uses the production owner-host entry point, not a side example. Evidence compares legacy and owner execution, native saves, identities, accounting, ordering and RNG continuation. Existing missing C behavior stays explicitly in its parity backlog; mandatory architecture lifetime/accounting invariants cannot be deferred there. M6 migrates remaining default production command/domain paths and completes default-host composition beyond this slice; M7 performs final aggregate verification. M5 cannot claim A2/A4/A6 from separate reduced demonstrations or defer any selected-wand collaborator to M6.

## I3 execution specification (2026-10-10)

Implement one lifetime module with bounded capabilities and the existing sealed Host publication. A finalized survivor is the same-generation item explicitly retained by an admitted operation. None, stale/retired and nonnegative survivors are no-ops. Preserve the current initial nonpositive empty-wand refusal; this migration does not activate missing C wrest/backfire branches. Connect finalization to production wand completion and a bounded legacy adapter after its gameplay borrows end, preserving public Vec-returning APIs and using one algorithm.

Validate all accounting and represented references before publication. Prepare Objects retirement, Equipment matching wielded/armor/dressing cleanup, Hero matching quiver cleanup, Clock matching active/cached timer removal, and Billing live-to-consumed transfer as exact declared participants. Geography observations authenticate cached residency. Existing single-participant CommandAccounting cannot certify this operation; add its exact multi-participant validator under the existing Host protocol. Preserve unrelated references/timers, tie/deferred ordering, quantity, receipt order and unknown terms. No partial cleanup or dust/event emission on refusal; one successful publication emits its event once.

Reuse existing disposal/accounting mechanics and old public disposal refusal contract. Definitions, host/generation/residency/revision/grant/schema provenance remain authenticated. Avoid whole-owner requests, World reconstruction and a second transaction engine. Retire quantity-one/three whole identities, retain consumed billing evidence, and round-trip active/cached fixtures through native save/restore. Cross positive/refusal fixtures required by I3 remain mandatory; no substep claims full I3 acceptance. This specification is implemented and independently reviewed together with the coherent unit.

## I3 acceptance and execution adjustments (2026-10-10)

I3 is accepted with one shared bounded lifetime algorithm, borrowed legacy and five-owner native publication, active/cached accounting and timer preservation, and historical V1/V2/V3 reconstruction. An independent wiring review found the V3 gameplay facade guard defect; the repaired guard and complete reply/state regression passed a separate readback. The final frozen-source run passed all 12 gates, 1491 workspace tests and all three unchanged goldens. Earlier incomplete reviews and the first static-check naming collision remain failed attempts in the evidence checkpoint; they are not acceptance evidence. I4/I5/M6/M7 and the separate 53-family parity backlog remain open.

For subsequent units, run cheap formatting/static/generated-data checks before expensive workspace gates. Freeze the candidate before review. Bound each review to a concrete production slice, omitting repeated moved definitions and unrelated fixtures; split large candidates into complementary reviews whose interaction seams are explicit. Read chunks individually within the tool-output bound and reserve command/token headroom for the verdict. Reuse completed reviews for unchanged code; review repairs as exact deltas. Resource stops without a verdict stay failures. Keep one writer and one Cargo allocation, and retain complete full-gate/source-binding proof before commit. Root quota usage remains unknown; no weekly-limit savings are claimed.


## I4 execution specification (2026-10-10)

The opt-in extension profile approves a finite immutable guest catalog. Guest modules load dormant after the nine NetHack owners are populated; live attachment changes the action-policy capability used by future wand commands. The policy returns a bounded typed action and has no owner dependencies. The shared gameplay algorithm still performs selection, RNG, publication, lifetime and accounting. Read-only kernel attach/detach admission precedes the session mutation boundary, so duplicate or missing attachment refuses without changing its checkpoint. Historical profiles retain their original manifests, code identities, schemas and guards.

The separate workflow profile uses distinct caller, provider and random executable identities. `pause_wand` captures the actual dependency pipeline and suspends before gameplay effects. `approve_live_wands` explicitly approves detached observations of the trusted host's live execution before incoming saved input exists. The resulting private approval record contains no runtime aliases; fresh factories receive fresh owner handles and compare complete operation contexts, frames and participants against this independently supplied record. Missing workflow approval supplies an empty restore codec catalog. `revoke_wand_workflow_approval` withdraws that restore authority without changing owner payloads.

An application must retain the approval independently and supply it through `with_wand_workflow_approval` for reconstruction. This unit does not implement a durable authenticated approval store or infer authority from the game save. Approval must be refreshed deliberately when the approved execution checkpoint changes. The ordinary public gameplay reply API is unchanged; the new workflow resume API returns a bounded success summary. Existing default workflow bounds and guest memory bounds are unchanged.

The integration witness pauses an ordinary charged wand, approves its actual live execution, attaches a cancel guest, captures, drops the source host and reconstructs a fresh owner composition. The old ticket refuses; a fresh ticket resumes the retained original policy and consumes one charge exactly once. A subsequent ordinary command executes the attached guest and leaves native world bytes unchanged. Additional exact-checkpoint witnesses cover missing/mismatched executable approval, absent workflow codec/approval, live revocation, incompatible owner schema and unsupported native migration. I5 must still combine this with the broader effect/lifetime/equipment/timer/shop/native compatibility scenario; I4 does not close M5 or full C parity.


## I4 acceptance (2026-10-10)

I4 is accepted with executable guest activation on a populated owner host and a retained preattachment workflow that survives source-host destruction and fresh reconstruction through an independent live approval record. Six I4 native integration tests, seven historical I3 regressions, 31 workflow tests and 65 persistence tests pass. Completed complementary Astra reviews found no required changes. The original incomplete assembled review remains recorded as a packaging failure, never a PASS. The final frozen-source run passes all 12 gates, 1498 workspace tests and all three unchanged goldens on 542 non-documentation inputs. No dependencies, golden constants, existing owner formats or default limits changed.

The next unit is I5, the combined production acceptance scenario. M6, M7 and the separate 53-family C parity inventory remain open. The runtime goal remains observed paused; the user-authorized continuation and review were performed manually without claiming full-goal completion.

## I5 aggregate checkpoint and remaining acceptance

I5a adds one populated production `OwnerBridge` suite in
`runtime/tests/wand_aggregate.rs`. It combines a real selected effect with a
borrowed legacy state/RNG oracle, independently approved retained execution after
source drop and reconstruction, executable cancellation of future commands,
stale ticket/request/read refusals, repeated bill-term refusal, and five-owner
retirement of a separate negative object. It preserves unrelated equipment,
filters matching timers, retains the complete consumed bill record, and checks
both owner checkpoint restoration and legacy native import. Native import
reinserts tied timers before equals; owner checkpoints preserve exact queue order.
The suite compares each route with its own complete expected state and continues
four authenticated raw draws on both routes.

This is supported aggregate evidence, **not full I5 acceptance**. Astra's bounded
independent review passed the added fixture and identified the following remaining
requirements. Existing I1–I4 refusal fixtures remain required.

| Ordered substep | Concrete delivery | Acceptance boundary |
|---|---|---|
| I5a | Aggregate supported-command fixture and compatibility checkpoint | Accept the fixture after full gates; keep I5/A6 open. |
| I5b | Versioned retained completion reply/events, authentic stale observation/publication replay and consumed-shop-evidence replay | Compare retained ordered events with the borrowed handler. Reject actual replay inputs before new effects/draws. Preserve old boolean-only workflow artifacts/checkpoints; do not invent missing historical events. A public guarded request replay alone is insufficient. |
| I5c | Source/RNG contract and explicitly opted-in executable charge-attempt module, then the same selected identity through effect and five-owner retirement | Current `PositiveCharge` cannot produce a negative survivor. Proposal vocabulary alone is insufficient. Use authenticated Objects/Random ports and existing lifetime publication; preserve default positive-only behavior and frozen goldens. Do not substitute a restored negative image or a custom non-C depletion rule for the promised legacy-compatible chain. |
| I5d | Actual separate-process reconstruction witness and final combined positive/refusal suite | Persist separately trusted host approval independently of incoming game saves; no approval inferred from the save and no durable authenticated store claim without implementation. Run final gates once on frozen reviewed source before accepting I5. |

I5b–I5d are narrow integration work, not a repeat audit of accepted modules or a
kernel rewrite. Root owns the sole writer/Cargo lane. Each review receives only
changed code and required contract excerpts. Prefer supplying sealed source chunks
directly in the initial review prompt to avoid repeated tool/context overhead.
When a file read is needed, chunks stay below 1500 tokens with one read per tool call. Preserve actual phase usage and incomplete verdicts;
root usage remains unknown. Full architecture acceptance, M6/M7 and the separate
C-parity inventory remain open.

After the I5a checkpoint, implement I5b–I5d in dependency order with focused meaningful checks and bounded review packets. Batch their full workspace/golden verification at the final frozen integration boundary; avoid a complete workspace run between local substeps. Each commit still requires all mandatory gates.

I5a checkpoint verification: 69 scoped tests, independent Astra change PASS, 1499 workspace tests, all 12 required gates and all three unchanged goldens. The first gate stopped before workspace on redundant borrows of Copy Scheduler; the exact reversible Clippy correction changes no assertions. Final frozen input digest `10ae2145b85aeed9d46818f229cb06dccc57e126e9d27043d58f9544fd38af8c` covers 543 non-documentation files. Strict I5 remains partial; this checkpoint does not advance M6.

## I5b reviewed implementation (2026-10-10)

The optional detailed-reply profile retains canonical completion metadata and ordered reply chunks within existing workflow bounds. Full outcome framing is included in the limit check. Overflow returns an explicit detailed-read Bounds error while preserving the actual boolean summary and committed gameplay prefix. Historical I4 profiles keep their exact executable identities and boolean protocol; a detailed read refuses before consuming their ticket.

The actual public lifetime finalizer now provides a test-only replay witness: reuse of its consumed opaque batch, its authenticated prepublication billing observations, and the original complete consumed-shop retirement proposal all refuse without changing any of nine owner payloads or the complete receipt history. The proposal refuses at the removed object boundary; this does not independently prove duplicate billing rejection after successful object admission. Production staging still commits immediately.

Astra passed I5b with 74 scoped tests and targeted Clippy passing. Full integration gates remain pending until I5c/I5d finish; no I5 acceptance or new commit is claimed. Review evidence is recorded in the microkernel goal ledger.

## I5c implementation boundary

The new policy is explicitly optional and has distinct executable/profile bindings. Historical positive-only behavior remains the default. Its first supported slice is a canonical uncursed wand of fire in the explicitly admitted ordinary context; unsupported new-policy definitions, cursed or context cases refuse before RNG rather than silently borrowing incomplete backfire semantics. Full P11a parity remains separate.

At zero charges, Random previews exactly the existing Below(121) sampler using a private local clone and returns an authenticated bounded observation, including the current reservation and sampled value. The preview changes no live state. The coordinator stages the real Sample candidate and validates its proposed reply against that observation. Objects validates the same selected generation and the existing signed charge formula against the authenticated observation. The successful wrest publishes Random and Objects together; failed wrest publishes Random with Objects as authenticated readonly evidence. Nonzero charges do not acquire a charge draw. Existing owner accounting validates the exact transition and participants; no Random state crosses into Objects and no diagnostic receipt becomes authority.

The shared effect continuation uses the already accepted charge result once. Successful wrest emits the source message before the effect and carries the same selected identity at charge -1 into existing five-owner lifetime publication. Failed zero attempts retain the item; negative attempts skip effects and proceed to terminal cleanup. Tests must cover authentic RNG continuation, success/failure, stale preview/publication refusal, same-object effects and complete retirement, and default isolation. No kernel concepts, limits or save-owner formats are widened.

## I5d process and trust boundary

The witness must launch actual producer and consumer OS processes with distinct PIDs. The producer emits owner-save bytes and a separately captured trusted live approval record. The consumer receives approval through an explicit host-controlled input, independently of incoming save bytes, then reconstructs owners and resumes with a fresh ticket. Missing approval and a changed approved checkpoint must refuse. Positive resume must compare retained ordered events, complete native state and continued RNG against the producer's independent expected output; repeated execution must refuse without new effects.

A bounded host-record export/import interface may transport detached native approval data, with canonical encoding and exact reconstruction checks. Import is an explicit application trust decision, never authentication of arbitrary bytes and never inferred from game saves. The test uses separately controlled files; it does not establish a durable authenticated approval store. Final aggregate tests and all mandatory gates run once on frozen independently reviewed source before the integration commit.

The review runner now accepts an optional directly hash-bound source packet, archived and supplied once with the initial instructions. Instructions remain limited to 1000 tokens; source context is limited to half the existing phase input ceiling. Actual model input/output, command and wall limits remain enforced. Historical packets are unchanged. Archived source hash and token estimate are checked in the usage report. Thirty-three runner tests pass; this process change is included in the independent integration review.

## I5 integration acceptance (2026-10-10)

I5b–I5d are accepted together after independent Astra PASS, 80 scoped tests, 33 review-runner tests, all 12 gates, 1510 workspace tests and all three unchanged goldens. The frozen source digest `f1697e983d91e85ef536404e361a125182b9ccbf2fc1b873d0058dd38157df00` covers 548 non-documentation inputs. The detailed retained reply, authentic stale/publication/shop-evidence refusals, opted-in same-selected fire wrest/effect/five-owner retirement and actual separate-process reconstruction are now implemented and tested. Historical native/workflow profiles retain their original behavior; default wand behavior, golden constants, dependencies and kernel limits are unchanged.

This closes the scoped M5 architecture acceptance, including populated native extension and retained workflow integration. It does not close full P11a/C parity, general item naming, cursed/backfire behavior, other new-policy wand kinds or an authenticated durable approval store. M6 default domain migration and M7 final architecture audit remain.

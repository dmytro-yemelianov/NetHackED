# P06: one equipment occupation slot

Queued under P04–P10. Preserve the reviewed attribute/capacity/regeneration worktree and single-writer/Cargo ownership. Confirm model routing after the read-only equipment map: new occupation/timing ownership warrants Sol/high implementation; fresh Sol/high semantic review is required.

## Required exit

Provide one real wear/remove equipment slot with C-derived nonzero delay, cancellation and cursed-removal outcomes through a public player action path. Reuse owned item metadata and existing slots/occupation APIs where present. Missing slot/delay facts become an explicit extraction/ownership prerequisite, not a handwritten runtime fact table or display-name approximation.

Before edits, pin NetHack-5.0.0_Released slot eligibility, delay/turn boundaries, effect application/removal timing, interruptions and cursed outcomes. Any new persisted occupation state has one owner and compatible defaults/old-load tests. Prove public start/progress/complete/cancel/retry/cursed behavior, actual turn costs and save/RNG continuation. Preserve world-turn regeneration/affliction/corpse ownership and leave full equipment frontend/slot coverage outside this single-slot unit. P07 property source chains should be able to use the actual worn state.

No new dependencies/C execution/unchecked goldens/commits/coordinator docs by worker. Existing assertions remain unchanged except setup. Completion requires independent review and final complete gates/goldens, not helper-only equipment math.

## Discovered prerequisites and split

The read-only API map found no Wear/Remove actions, explicit worn-slot state or resumable occupation owner. Armor slot/AC metadata exists, but owned item definitions do not retain C `oc_delay`. P06 therefore has two ordered units; completing metadata alone does not satisfy P06.

| Subunit | Outcome | Routing |
|---|---|---|
| P06a owned delay facts | Extract selected released C delay facts into owned item definitions; preserve omitted-field legacy loading/canonical representation and test owned overrides | Sol/medium implementation, fresh Sol/high review |
| P06b public occupation | One source-verified armor slot through public wear/remove, real delayed effect timing, cancellation, cursed removal, persisted continuation and old-load compatibility | Sol/high implementation, fresh Sol/high review |

P06a first verifies extraction/schema ownership and C armor slot/delay eligibility, with no handwritten runtime fact table. Changes to generated vanilla metadata must be intentional and attributable; legacy missing-field/hash behavior must remain compatible. The coordinator records the verified slot and public occupation contract after the missing source facts are established. P07 must consume explicit worn state; merely carrying armor cannot establish equipment provenance.

## Source fact checkpoint and P06a contract

A bounded read-only Luna/high source scout identified `LEATHER_ARMOR`, delay 3 and `ARM_SUIT`/`W_ARM` in the selected released C source. Its slot/AC state starts at `setworn`; wear completion callbacks are delayed, and removal keeps the slot until `Armor_off`. C single-item dressing uses negative `multi` plus `afternmv`, rather than a resumable generic occupation. Cancellation is external: `stop_donning` unworns interrupted donning but preserves interrupted doffing; no user cancel command exists in that path. P06b must verify total turns and implement a real reachable interruption rather than inventing a user command. The one-slot exit remains required.

P06a owns extraction/generation, definition and owned lookup/schema changes needed to retain `oc_delay`, plus independent metadata tests. It must preserve absent-field old loads/canonical representation, explicit owned overrides and stable generated ordering. Confirm the exact selected C delay/slot and at least one zero-delay boundary; generated facts come from the existing reproducible extraction route, with no manually authored runtime table. Existing preprocessing may be used for facts; no C compilation or executable. No public occupation state yet. Preserve all reviewed P04/P05 source and tests.

## P06b public behavior contract to confirm after P06a

Use the source-verified suit slot with a delay greater than one turn. Before edits, audit total world-turn accounting and an existing reachable external interruption route, preferably item theft if already implemented. Required public tests start and finish wearing/removal, distinguish immediate worn/AC state from deferred callbacks, reject cursed removal with the source-correct cost, interrupt both directions with the C asymmetric rollback, retry, and continue mid-operation after a full save/load with identical RNG/events/state. A helper-only cancellation test or invented user cancel command does not satisfy the interruption exit. If the existing interruption path is absent, record and implement the smallest genuine source-derived prerequisite within this unit.

Define one persisted owner for worn slots and delayed dressing, with defaults and exact old-load coverage. Inventory ownership changes must keep worn/pending references valid. Relevant AC must consult the selected actual worn state. Source-confirm slot blockers/eligibility in represented ordinary contexts; broader slots/form cases remain explicit gaps. A carried item alone never proves wearing. All waits preserve affliction/hunger/regeneration/world-clock ownership, and saved operations resume exactly once. No new timer subsystem duplicating P08; P06 single-item negative-multi semantics are mapped to the existing action/world-turn API. New timing ownership warrants Sol/high implementation.

## P06a focused handoff

The existing extracted tables already retain `delay`; P06a therefore changes the Rust generation/owned definition/pack path. `oc_delay` defaults to zero and zero is omitted from full definition canonical JSON; an omitted patch inherits while explicit zero overrides. The writer reports 126 affected tests passing, focused lint/static/extraction/generation/fmt checks passing and prior dirty source preserved. The vanilla canonical delta is reported as exactly 132 added nonzero delay facts; the old omitted bundle hash remains exact. Fresh independent review and full gates are required before treating those reports as accepted evidence. P06b has not started.

## P06b reviewed implementation checkpoint

Sol/high implemented the public suit path and the genuine unworn-target theft prerequisite. Fresh Sol/high review found three defects in selected-suit transfer guards, theft MC/RNG and attack termination; the first repair closed them. Two added flee/teleport prerequisites needed a further narrow repair for the source draw ordering and fallback candidate predicate; both then passed independent review. All original assertions and the complete earlier input set are preserved. The one owner persists actual armor and delayed dressing, with source world-tick timing, absent-field migration, explicit-empty authority and full-save continuation tests. Broader worn-target disrobe, special teleport contexts, other slot frontends/forms and general armor properties remain explicit gaps. The complete integration gate is running; frozen goldens and final acceptance remain policy-dependent.

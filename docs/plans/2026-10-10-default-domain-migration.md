# M6 default domain migration

Draft execution scope following scoped M5 acceptance. This plan does not accept M6 or add missing NetHack C behavior. M6 migrates existing Rust behavior behind the accepted independent owners and authenticated module ports. M7 verifies the complete architecture after the default cutover. The separate 53-family C parity backlog remains unchanged.

## First unit: M6-I1 command frame without time advancement

The next implementation packet covers `CancelZapSelection` and the existing default dispatch frame's equipment reconciliation and derived armor class. Do not begin with `Wait`: it also requires monster rounds, passive ticks, timers, occupations and RNG ordering.

Read the named existing seams in `actions/mod.rs::step_player_action`, `actions/equipment.rs::reconcile_equipment` and `world.rs::recompute_hero_ac`. Extract shared bounded algorithms with Objects, Equipment, Hero and Actors ports; the legacy borrowed adapter remains an oracle. The new implementation belongs in command-frame/equipment modules and a separate `owner_persistence/runtime/default_commands.rs`, with the minimal owner-local command additions. It must not add another wand-specific catch-all, universal mutable context, World reconstruction during gameplay or mutable state mirror.

A distinct explicitly approved profile dispatches the supported cancellation frame and preserves historical profiles. Handle missing/dead hero behavior according to the current dispatcher. Until time/occupation orchestration is implemented, refuse helpless/dressing contexts that substitute Wait, and other unsupported actions, before publication. Represent committed reconciliation prefixes truthfully if a later bounded operation refuses.

Done means populated owner and borrowed legacy fixtures agree on complete native state and ordered events for ordinary cancellation, stale equipment and derived AC, and missing/dead hero cases. Cancellation advances neither time nor RNG. Authentic stale inputs and unsupported contexts refuse without unpublished state changes. Owner/native reload preserves identities and historical approved profiles. The actual implementation packet must bind the precise functions and smallest file allowlist before writing.

## Dependency order

These are ten migration groups, not a count of individual implementation tasks. Split a group only when its coherent code delta or verification exceeds one bounded packet. Passive clocks, monster rounds and full Wait acceptance are separate so no group depends on an incomplete future collaborator.

| Group | Depends on | Existing behavior to migrate | Completion evidence |
|---|---|---|---|
| M6-I1 | M5 | Cancellation frame, equipment reconciliation and derived AC | Complete state/events, no-time/no-RNG cancellation, genuine refusals and reload |
| M6-I2 | I1 | Passive tick algorithms in `turns.rs`, `ad_effects.rs`, `object_timers.rs` and their named collaborators | Existing affliction, timer, regeneration, hunger and occupation order through owned ports; no claim of full Wait yet |
| M6-I3 | I2 | `monsters.rs::step_monsters` and existing combat collaborators | Snapshot order, movement budgets, effects, death and RNG; bounded resumption without repeating attacks |
| M6-I4 | I2, I3 | Full default Wait and action-time orchestration | Zero/one/multiple rounds, exactly-once prefixes, pending continuation and native reload |
| M6-I5 | I1, I4 | Movement, doors, traps and inline Search/Untrap dispatch branches | Existing overload/refusal and impaired-direction RNG order; moved flag, traps and time match |
| M6-I6 | I1, I4 | Inventory, equipment, containers, quiver and ranged actions | Generations, quantities, references, debt, timers and occupations; reuse accepted lifetime accounting |
| M6-I7 | I6 | Existing item effects, engraving and remaining wand entry paths | Each existing dispatch branch has positive/refusal and continuation coverage; no new C mechanics |
| M6-I8 | I3, I6, I7 | Cast, prayer, sacrifice, economy and riding | Existing actor/hero consequences, billing and ordered time/RNG prefixes through owners |
| M6-I9 | I4–I8 | Stairs and active/cached residency traversal | Identity, timer, billing, admission and pending-work preservation across reload |
| M6-I10 | I1–I9 | Default dispatcher and consumer cutover | Every existing ActionAst branch reaches owner execution; initialize/import once, no gameplay World reconstruction fallback |

M7 follows I10: combined live-extension, retained-continuation, native compatibility and refusal evidence, unchanged goldens, final independent architecture review and obligation reconciliation. Optional wand integration alone cannot satisfy M6.

## Bounded agentic loop

Root owns integration and the sole writer/Cargo lane. Reuse the accepted architecture and inspect only the next group's named seams. The implementer receives one concrete behavior, exact source bindings, a small allowlist, acceptance fixtures and explicit exclusions. Normal implementation uses the existing `gpt-6.1-sol`/medium route; reserve `gpt-6-astra`/medium for independent coupled authority, continuation and persistence review. Apply the checked runner's phase ceilings and one repair allowance; incomplete or interrupted output remains pending, never PASS.

Supply only new code and necessary contract excerpts in a directly bound, archived source packet. Instructions remain at most 1000 tokens, inline source at most half the existing phase input ceiling, and actual input/output, commands and elapsed time remain guarded. Reuse completed reviews rather than rereading accepted modules. Run focused meaningful checks for each coherent unit; run all mandatory gates and unchanged goldens once on the reviewed frozen source before committing. If a group is split, document the remaining acceptance boundary rather than treating prerequisites as full closure. Do not turn implementation difficulty into a new architecture or parity audit.

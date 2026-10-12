# Full NetHack 5.0 parity: scope, evidence and execution map

Planning baseline: `6248cc6`, 2026-10-06. This is a scope and dependency assessment, not a fresh audit of every feature or authorization to implement every milestone. Reuse the [roadmap](../roadmap.md) and [542-row inventory](../parity/README.md). No production code changed during this assessment.

The [remaining-stream scope](2026-10-07-remaining-parity-scope.md) at `2f97135` now maps all 543 current inventory rows across seven streams, with 53 family cards, bounded reconciliation packets and catalog obligations. It carries forward accepted P04–P10 gaps and defines the next P11a audit. Family counts are planning structure; current-source audits establish concrete implementation tasks.

## Define the target

The working target is gameplay, content and player-command semantics against the vendored `NetHack-5.0.0/` tree, tag `NetHack-5.0.0_Released`. Pin the C build configuration and supported gameplay options before declaring conformance; conditional branches and calendar inputs matter.

| Layer | Required result | Scope decision |
|---|---|---|
| Gameplay and content | Matching state transitions, effects, probabilities, turn costs, commands, maps, quests and endings for the selected C configuration | Recommended baseline; currently the working assumption |
| Exact replay | Matching PRNG, seeding, bounded sampling, random-call order and generation, including the same-seed dungeon | Additional target; matching seed integers alone is insufficient |
| Original interface and file formats | C command/prompt behavior is already in gameplay; original window-port visuals, platform options and binary C save import/export add further obligations | Original presentation and C binary formats remain optional; Rust save continuation is already required |

Custom rule-pack extensions are tested separately from canonical vanilla conformance. Existing pack/save compatibility requirements still apply. A documented approximation inside the chosen target remains an open parity obligation.

## What the existing numbers establish

At the planning baseline (`6248cc6`, 2026-10-06), the checked-in parity tables contained 542 rows: 60 present, 189 partial, 290 missing, 3 unsure. The [inventory summary](../parity/README.md) reports subsequently refreshed statuses. Thus 482 rows are marked non-present. These are edited historical inventory statuses, not 482 validated remaining tasks at current HEAD and not a trustworthy current completion percentage. Rows have different sizes, some bundle multiple behaviors, and the inventory may omit additional C obligations. The initial roadmap snapshot 58/175/306 describes an older count. Refresh affected rows before estimating remaining effort.

Catalog extraction covers 383 species, 440 object kinds and 33 artifacts as data. That does not establish their behavior. [Behaviour coverage](../parity/behaviour-coverage.md) scans references:47/47 damage tags means they are named in engine code, not that every C side effect and context matches.

Recent independently accepted subsets include sampled monster HP, single-owner transient countdowns, custom monster spawning, supported item BUC/spe/quantity/poison, ordinary containers, typed ice-box corpses/age preservation, random existing-object artifacts and owned custom-item selection. [Acceptance records](2026-10-06-item-initialization.md) and [G1](2026-10-06-owned-item-generation.md) define their exact limits. Latest same-source acceptance: 770 Rust tests, three goldens and 12 gates. These goldens establish Rust regression determinism, not equivalence to C.

At the planning baseline, current-source checks also found stale claims: all six attributes and XP already exist (`crates/nethacked-sim/src/world.rs`, `progress.rs`), while old prose says attributes are absent. Full attribute formula integration still needs verification. Actor records then lacked stable species identity even though corpse/artifact identities existed. [P03](2026-10-07-actor-identity.md) subsequently adds persisted identity and a bounded definition-resolution slice; direct name-based behavior remains follow-up work. Dungeon topology requires comparison with `dat/dungeon.lua`; current abbreviated branches and synthetic maps cannot establish C world parity. Reusing the inventory requires correcting these contradictions without promoting broad rows on the strength of one repaired subcase.

## Complete workstreams

| Workstream | Obligations to verify and close | Main prerequisites |
|---|---|---|
| Evidence and state foundations | C feature/branch inventory, stable entity identities, property sources, scheduling/occupations, persistent world state, randomness contract and independent fixtures | Pin version/configuration; reconcile historical evidence |
| Hero creation and progression | All 13 playable roles and valid race/gender/alignment choices, starting kits/skills, XP/level gain/drain, HP/Pw, attributes, regeneration and conduct | Persistent identity/state, attributes and turn accounting |
| Equipment and properties | Wear/remove/wield slots, curse restrictions and delays, encumbrance, launchers/multishot, resistance/reflection/free action and other sourced intrinsics | Attributes, occupations, object ownership and identity |
| Combat and monster lifecycle | Complete to-hit/damage modifiers, every AT/AD context and passive effect, magic cancellation/MR, beams/reflection, species HP adjustment, groups/spawns, speed/AI/senses, inventories/item use, pets and monster-vs-monster combat | Properties, identities, time, object effects and generation |
| Object lifecycle and effects | Identification/appearances/knowledge/naming, stack split/merge, full mksobj fields/payloads, rot/revival/hatching/fuel timers, food/corpse effects, every wand/potion/scroll/book/ring/amulet/tool, wishing/artifact powers/gifts | Shared properties, occupations, inventories, event/timer state |
| Hero special systems | Spell learning/memory/failure/energy, skill practice/enhancement, prayer/sacrifice/troubles, polymorph/lycanthropy, timed conditions, riding/music/engraving | Attributes, properties, object effects and combat contexts |
| Dungeon and NPC content | `dungeon.lua` topology and all scoped special maps/variants, rooms/terrain/traps/vision/lighting, shops/priests/guards, role quests, Wizard behavior, Planes/invocation/ascension | Generation, monster AI/inventory, object effects and world persistence |
| Commands, interface and game flow | Every scoped command through intended frontends, prompts/cancellation/repetition/occupations, inventory/look/map memory/messages/status, death/disclosure/scoring/save/restore and endings | Integrate reachability and turn-cost tests with each feature |
| Text, options and calendar | Quest/Oracle/rumor text, calendar effects, enlightenment/overview, conduct/achievements, gameplay options and bones | World/NPC state, reproducible external inputs and frontend reachability |

The M1–M8 groups in the existing roadmap remain useful release groupings. Their old time estimates are not validated forecasts. These workstreams overlap; completing item initialization alone does not close object effects, combat or dungeon content.

## How parity will be established

Maintain an obligation register per feature: C symbol/configuration and branches; owned Rust entry points; required fields and dependencies; reachable frontend actions; independent expected fixtures; completed and excluded cases. Separate “implemented”, “source-conformance tested” and “verified through public play”. Record uncertainty rather than infer behavior from tags, declarations or passing goldens.

The binding [handover](2026-10-06-handover.md) says “No C at runtime, anywhere” and treats C as facts only. The default plan preserves that policy: derive reviewable fixture expectations from pinned C source, including prescribed random draws and ordered bounds, and independently review them. Compare state, events, quantities, failures, probabilities, draw order and turn costs; test save continuation and actual public integration. Use properties/distribution checks as additional evidence. Lean models help where their relationship to Rust and C is established; their existence alone does not prove engine parity.

A separate development C differential executable would provide stronger empirical comparison, but it is **not authorized by this plan**: its execution conflicts with the strict facts-only reading and requires an explicit policy decision before adoption. Do not disguise it as a production exception. No such executable or dependency was added.

Rust currently uses ChaCha8; C `rnd.c` uses configured ISAAC64 or platform Rand. Exact replay additionally requires choosing the C platform/backend, porting its stream/seeding and sampling semantics, and matching all call sites. Gameplay fixtures with prescribed randomness can proceed without claiming same-seed equivalence.

To close a feature: cover its scoped C branches and cross-system interactions, connect public actions, preserve save behavior, independently review, and run all gates/goldens. Goldens change only under a recorded task-specific intentional-change policy. Full parity requires all obligations in the chosen scope closed, unresolved inventory questions addressed, and no remaining scoped divergences. Safety fallback caps, synthetic levels and omitted rare branches must be proved irrelevant to valid vanilla cases or remain explicit blockers to exact conformance. No universal mathematical equivalence claim follows from finite tests.

## First ten bounded units

These are proposed contracts, not findings that each subsystem is entirely missing:

1. Reconcile rows affected by recent commits and stale attribute/AD claims; add branch obligations and test references without unsupported green promotion.
2. Establish the source-derived fixture/verification format on existing `newmonhp` cases; keep C execution disabled.
3. Add persistent canonical/custom actor species identity, with renamed-actor behavior and save/load tests.
4. Audit/connect stored attributes to C hit/damage and burden formulas, with independently derived boundaries.
5. Audit/add natural HP/Pw regeneration under explicitly selected conditions.
6. Complete one wear/remove occupation, delay, cancellation and cursed-removal path.
7. Complete one property-source chain, such as free action through equipment, attack outcomes and saves.
8. Implement a bounded corpse rot/revival lifecycle contract, including nested ice-box/cache suspension and resumption.
9. Audit/complete monster speed and movement-energy action allocation, including interactions with hero timing.
10. Audit/complete one ordinary monster group creation path with difficulty, placement and genocide cases.

Then expand combat/effect families, inventories/AI/pets, full class effects, role/quest/level variants and command/text coverage. Each unit gets a small source-evidenced contract; avoid broad “port all combat” assignments.

Use the [full-parity execution loops](2026-10-07-full-parity-agentic-loops.md), which refine the [existing agentic loop](2026-10-06-agentic-loops.md): one Sol/medium implementer, fresh Sol/high semantic reviewer, deterministic local verification, bounded repair limits and coordinator acceptance. No fleet or duplicate discovery. Count remaining acceptance units only after their obligations/dependencies are scoped; no reliable total time or token estimate is available yet.

## Planning acceptance

Independent Sol/medium planning review PASS: major domains, dependency order, bounded units, evidence limits and facts-only policy are consistent. Inventory exploration used one Sol/medium read-only worker; no agent fleet or implementation patch. The optional scope response has not arrived; the recommended working target above is an assumption, not a recorded user choice.

Documentation links, whitespace and generated parity-summary checks pass. Production source and Cargo.lock are unchanged, so the accepted G1 full gate (770tests/all3goldens/all12checks) is reused rather than repeated. This planning commit does not refresh every historical row, claim a current completion percentage, introduce a C executable or begin the proposed backlog.

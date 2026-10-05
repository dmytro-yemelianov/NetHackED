# NetHack Fidelity D1 — Core Formulas & Lean Models — Design

Date: 2026-10-04
Status: Design approved in conversation
Branch: `feat/fidelity-d1` (from `chore/repo-hygiene`)
C reference (formulas with `NetHack-5.0.0/` file:line citations, current Rust/Lean/proptest locations):
[2026-10-04-fidelity-d1-c-reference.md](2026-10-04-fidelity-d1-c-reference.md) — binding source for exact formulas.

## Goal

Make the core mechanics in `nethacked-core` (and their callers in `nethacked-sim`) follow NetHack 5.0 C
for the 16 items below, and update the Lean 4 models, theorems and proptests so they describe the
corrected behaviour. Fix the vacuous/weak Lean theorems found in the review.

## Non-goals (D2/D3)

Monster/item data tables, AI behaviour/peaceful flags, floating-eye passive, hero XP/level-up,
pets following across levels, monster damage dice, applying encumbrance to movement in the sim,
telepathy-only-while-blind, abon()/attribute system (abon treated as 0).

## Principles

1. **Explicit rolls.** Core functions stay pure; every random draw is a parameter named after its
   C distribution (`d20: u32` = rnd(20) ∈ 1..=20, `roll_343: u32` = rn2(343) ∈ 0..=342, `rn2_5`,
   …). Functions document the valid range and treat out-of-range rolls by clamping (never panic).
   `nethacked-sim` draws rolls from `SimulationWorld.rng`.
2. **Lean mirrors Rust.** Each Lean model takes the same roll arguments; theorems are universally
   quantified over rolls in range. Theorems that become false are restated (not deleted silently);
   the Lean guide's theorem table is updated accordingly.
3. **Proptests compare against an independent reference.** For each changed mechanic, a proptest
   checks the Rust function against a small reference implementation written directly from the C
   rule (in the test file), over random inputs *and* rolls. Tautological proptests that re-implement
   the code under test are removed or rewritten.
4. **C citations.** Each changed Rust function's doc comment cites the C source (`file.c:line`).
5. **Docs.** `docs/formal-mechanics-spec.md` "Known divergences" loses each fixed item;
   `docs/lean4-verification-guide.md` theorem table updated.

## Mechanics (see C reference for exact formulas)

| # | Mechanic | Rust (core + sim) | Lean |
|---|---|---|---|
| 1 | Hero melee to-hit | `tmp = 1 + abon(0) + target_ac + level + luck_bonus + enchant + skill_hit`; hit iff `d20 < tmp`; bare-handed unskilled bonus per C | `Combat.lean` restated; monotone in AC/level/luck |
| 2 | Damage & AC | min 1 on hit; `rnd(-ac)` reduction only when hero is defender (`absorb_roll` param); no reduction on hero→monster | `calculateDamage` with absorb roll; damage ≥ 1 |
| 3 | Floor traps | C floor set; flying/levitation avoids them; web not avoided; seen trap escape iff `rn2_5 == 0` | `isFloorTrap` table; existing theorems re-proved |
| 4 | Luck timeout | blessed/uncursed/cursed luckstone rules; period 300 with Amulet or angry god else 600 | restate `luckstone_heals_negative_luck` (blessed only); uncursed freezes |
| 5 | Hunger | nutrition `i32`; strict C boundaries; Starved < −(100+10·Con) | monotonicity over `Int` |
| 6 | Encumbrance | `weight_cap(str, con, levitating, wounded_legs)`; tier `min((wt−cap)·2/cap + 1, 5)`, 0 when wt ≤ cap | tier theorem; cap ≤ 1000 |
| 7 | Enchant armor/weapon | armor `s>3` (elven 5) evaporation iff `rn2_s != 0`, gain `rnd(...)`; weapon >+5 evaporates with prob 2/3; blessed weapon gain `rnd(3−spe/3)` | restate `enchant_below_cap_safe`, `_increases` |
| 8 | Wand recharge | explode iff `n>0 && n³ > roll_343` (wishing: any recharge); charges `max(spe+1, roll)`; recharge count stored in a dedicated `recharged` field, not `erosion` | replace both recharge theorems |
| 9 | Shop prices | C charisma table; ×4/3 surcharges (dunce cap, low-level tourist, visible shirt; unidentified random surcharge as roll param); sell = base/2 (or /3 roll), CHA/curse-independent | restate `sellFactor`, re-prove `sell_le_buy` |
| 10 | Bag of Holding | recursive `mbag_explodes(item, depth, roll)` incl. charged cancellation wand, bag of tricks | keep `boh_cannot_contain_boh`; depth-0 explodes; cancellation explodes |
| 11 | Polymorph overkill | excess discarded on rehumanize; rehumanize never kills (except Unchanging) | simplified theorems; proptest changed |
| 12 | Bones cursing | curse iff `rn2_5 != 0`; quest items always | restate `corrupt_buc_always_cursed` |
| 13 | Mysterious Force | P = 1/(4+mf), mf increments on trigger; push `rnd(rn2(3+align))` (0 ⇒ same-level teleport); disabled in bottom 4 levels; sim tracks `mf`; existing Sanctum clamp kept | new `MysteriousForce.lean` bounds |
| 14 | Priest protection | C donation formula; first purchase 2–4, then +1 to 9, then +1 w.p. 1/protection up to 20 | `priest_protection_bounded` ≤ 20 |
| 15 | Quest roles | C leader/nemesis for the 9 roles (Rogue: leader Master of Thieves, nemesis Master Assassin; Tourist nemesis Master of Thieves) | — |
| 16 | Weak Lean theorems | — | `HasLOS.Adjacent` requires Chebyshev distance 1; LOS symmetry theorem; multi-step beam termination (≤ E steps) and pathfinding convergence (≤ D steps) |

## Data-model changes

- Wand recharge count: a dedicated field (e.g. `ItemRecord.recharged: u8`, `#[serde(default)]`)
  replaces the `erosion` overload in `nethacked-sim`; all readers/writers updated.
- `SimulationWorld`: `mysterious_force_count: u32` (`#[serde(default)]`).
- Nutrition type becomes `i32` where it flows through core hunger APIs (sim field adapted).

## Testing

- Each mechanic: unit tests at C boundary values + a reference-model proptest.
- Sim integration: existing tests adapted only via setup (rolls now random); new sim tests for
  wand recharge explosion odds bucket, bones curse ratio over seeds, mysterious-force never in the
  bottom 4 levels and never into the Sanctum, quest data per role.
- `lake build` passes; no `sorry`/`admit`/new axioms (check with the existing audit approach:
  `#print axioms` on changed theorems).
- CI set: fmt, clippy -D warnings, tests --locked, wasm32 build, doc links, Lean job.

## Risks

- Many existing sim tests assume old deterministic outcomes; they are fixed by setup or seed
  choice, never by weakening the asserted behaviour.
- Lean proof effort for LOS symmetry (M) and multi-step theorems (S each); if LOS symmetry proves
  too costly, the relation definition fix still lands and symmetry is recorded as planned.

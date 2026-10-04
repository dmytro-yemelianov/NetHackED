# NetHack Fidelity D2 — Data Tables Used by the Simulation — Design

Date: 2026-10-04
Status: Approved by delegation ("do the proper specs, planning and go")
Branch: `feat/fidelity-d2` (from `main`)
C reference (binding values with `NetHack-5.0.0/` file:line citations and current mismatches):
[2026-10-04-fidelity-d2-c-reference.md](2026-10-04-fidelity-d2-c-reference.md)

## Goal

Make monster and item data match NetHack 5.0 C and make the simulation actually use it:
monster attacks with their dice, hero weapon damage from weapon dice, armor AC from armor data,
peaceful monsters, corrected bestiary/item/pantheon tables.

## Non-goals (D3)

Floating-eye passive paralysis and other "helpless turns" effects; per-monster speed in the
scheduler; hero XP/level-up; pets following across levels; encumbrance applied to movement;
telepathy only while blind; item kinds on records (name lookup stays the mechanism here).

## Principles (inherited from D1)

- Explicit rolls in pure `netrust-core` fns; `netrust-sim` draws from the world RNG, each roll once,
  only when C draws it.
- Lean models and theorems updated where a modelled formula changes; no `sorry`/`admit`/new axioms/
  `native_decide`; `#print axioms` audit.
- Reference-model proptests for changed formulas.
- C citations against the vendored tree. `docs/formal-mechanics-spec.md` "Known divergences" kept
  truthful; `docs/lean4-verification-guide.md` two-table rule.
- Saves stay loadable: new record fields use `#[serde(default)]`.

## Design

### 1. Bestiary data (`netrust-data/src/monsters.rs`)

- Every BESTIARY entry gets C values: class letter (`glyph`), level, speed, AC, alignment, size,
  and an **attack list** replacing `damage_dice`:
  `attacks: &'static [Attack]` with `Attack { at: AttackType, ad: DamageType, n: u8, d: u8 }`.
  `AttackType` covers the AT_ kinds present in the table (Bite, Claw, Weapon, Touch, Butt, Kick,
  Sting, Hug, Breath, Gaze, Magic, None); `DamageType` covers the AD_ kinds present (Phys, Fire,
  Cold, Elec, Sleep, Poison(Str/Dex/Con as DrainStr), Acid, Paralyze, Stone, Drain, Curse, ...).
- Flags needed by D2: `peaceful_by_default` (M2_PEACEFUL), `is_human`, `is_unique` (correct
  capitalisation: "Medusa"), `mindless`, `size`.
- Invented entries replaced by C ones: "war dog" → "large dog" (`MonsterSpeciesId::LargeDog`;
  update Lean `PetCoop.lean` names), invented abilities removed (floating-eye active gaze, breath for
  Surtur / Minion of Huhetotl), quest guardian becomes per-role C guardian names (see §5).
- Master Assassin / Master of Thieves group placement fixed; "The Norn"/"The Dark One" keep
  BESTIARY names consistent with the quest table (D1 Task 11).
- Lookup helper `monster_archetype_by_name(&str) -> Option<&'static MonsterArchetype>` (case-
  insensitive) — the sim maps actors to archetypes by name (documented divergence: no species id on
  records until D3).

### 2. Item catalog (`netrust-data/src/items.rs`)

- Every ITEM_CATALOG entry gets C cost, weight, damage small/large (weapons only; non-weapons 0),
  AC (armor), `oc_magic` flag, wand `direction: WandDir {NoDir, Immediate, Ray}`, food nutrition.
- Wands of digging/teleportation no longer deal beam damage (C: dig / teleport effects; NetRust
  keeps their non-damage effects as they are or a no-op message, documented).

### 3. Combat uses the data (`netrust-core/src/combat.rs`, `netrust-sim/src/combat.rs`, `monsters.rs`)

- **Monster → hero:** each melee-type attack in the monster's list is resolved in order (C
  mattacku): to-hit per attack (D1 formula), damage `d(n, d)` rolled per hit, AC absorb (D1),
  damage-type handling: Phys → HP; Fire/Cold/Elec/Sleep/Poison/Acid → 0 HP if the hero has the
  matching intrinsic resistance else HP damage (special side-effects deferred, documented).
  Breath/gaze/magic attacks keep existing ability handling but use C dice. AT_NONE (passive) is not
  an active attack.
- **Monster → monster (pets):** C `mhitm.c` to-hit `find_mac(mdef) + m_lev` vs `rnd(20+i)` (no +10)
  and per-attack dice.
- **Hero weapon damage:** C `dmgval`: `rnd(small die)` vs small targets, `rnd(large die)` vs large
  targets (`size >= MZ_LARGE`), plus enchantment and skill damage bonus; bare hands `rnd(2)`
  (martial arts `rnd(4)` for Monk + skill bonus per C).
- Lean: `Combat.lean` damage model generalised to a variable die size (`base_roll ∈ 1..=die`),
  damage ≥ 1 theorem kept.

### 4. Armor AC (`netrust-core`, `netrust-sim`, `netrust-data/src/roles.rs`)

- Hero AC = 10 − Σ over worn armor of `a_ac + spe − min(erosion, a_ac)` (C `find_ac` /
  `ARM_BONUS`), ring/protection bonus (`divine_protection`) subtracted as C `u.ublessed`.
- "Worn" = carried armor items, at most one per slot (body, cloak, shield, helm, gloves, boots) —
  slot from the item name/kind table; extra carried pieces of the same slot don't stack
  (documented approximation until equipment slots exist in D3).
- Role base AC in `roles.rs` drops hardcoded armor; starting inventories gain the C starting
  armor where NetRust has the item kind; AC recomputed on pickup/drop/creation.
- Monster defender AC = archetype AC (data) − carried armor enchantment as today.
- Lean: new `ArmorClass.lean` (or within `Combat.lean`): `findAc` model + theorems (more armor never
  increases AC; AC ≤ 10 with no negative-enchanted armor).

### 5. Peacefulness (`netrust-arena` ActorRecord, `netrust-sim`, `netrust-core/src/engraving.rs`)

- `ActorRecord.is_peaceful: bool` (`#[serde(default)]`), set at spawn by
  `peace_minded(archetype, hero_alignment, hero_align_record, roll)` per C `makemon.c`:
  always peaceful (`peaceful_by_default`: shopkeepers, temple priests, watchmen, your quest
  leader/guardians), always hostile cases, else alignment-sign rule with the C random component.
- Peaceful monsters do not approach to attack or attack; the hero attacking a peaceful monster
  makes it hostile (and C alignment penalty for murdering peacefuls where applicable, documented if
  simplified). Shopkeeper "anger" uses `is_peaceful = false` instead of flipping alignment.
- Quest leader/guardians: peaceful, not tame (no pet behaviour). Guardian names per role from C.
- Elbereth (C `onscary`): does not scare @ humans, minotaurs, shopkeepers/guards/priests in their
  domain, the Riders; scares only when the monster would move onto/attack from the square — per C
  semantics as far as the sim's movement allows; Lean `Engraving.lean` scare predicate gains the
  exemption parameters.

### 6. Small fixes

- Rogue pantheon Issek / Mog / Kos (and verify all 9 roles' pantheons vs `role.c`).
- Breath weapon damage uses C dice per monster (`d(n,d)` from the attack entry).

## Testing

- Data tables: table-driven tests comparing every BESTIARY / ITEM_CATALOG entry to a C-derived
  expected table in the test (values transcribed from the C reference); class-letter test for
  genocide (`monster_class_of`).
- Combat: unit + reference proptests for per-attack resolution, dmgval, find_ac; sim tests (jackal
  bite 1d2 never exceeds 2 net of AC; Medusa / dragon multi-attack sequences; long sword d8 vs d12
  on large).
- Peacefulness: sim tests (Minetown watchman/priest don't attack; attacking one makes it hostile;
  quest leader peaceful not tame; Elbereth doesn't scare a shopkeeper).
- CI set + `lake build` + axioms audit.

## Risks

- Large existing test churn (monster stats, AC values, damage ranges): fix via setup/expectation
  updates that encode C behaviour, never by weakening assertions; list adjusted tests per commit.
- Balance shift (monsters now hit with their real attacks; hero AC from armor): intended.

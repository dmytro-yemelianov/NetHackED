# NetHack Fidelity D3 Technical Design: Alignment Systems

## 1. Overview and Objectives

Deliverable 3 (D3) eliminates two major documented divergences between NetRust and NetHack 5.0 C:
1. **Per-Role Initial Alignment Record**:
   Replace the hardcoded `INITIAL_ALIGNMENT_RECORD = 25` with NetHack's role-specific `initrecord` (10 for Archeologist, Barbarian, Healer, Knight, Monk, Rogue; 0 for Tourist, Valkyrie, Wizard).
2. **Canonical Monster Malign and Kill Adjustments**:
   Implement C `set_malign` (`makemon.c:2320-2366`) and death alignment resolution (`mon.c:3676-3726`), providing dynamic alignment gains and penalties on monster kills.

---

## 2. Architecture & Data Flow

```
                      +-----------------------------+
                      | NetHack C Sources           |
                      | you.h, role.c, makemon.c,   |
                      | attrib.c, mon.c             |
                      +--------------+--------------+
                                     |
                                     v
+-----------------------+   +-----------------------+   +-----------------------+
| netrust-data          |   | netrust-core          |   | netrust-arena         |
| RoleSpec.initrecord   |   | calculate_malign      |   | ActorRecord.malign    |
| RoleDef.initrecord    |   | adjalign, alignlim    |   | QuestState            |
| MonsterDef.maligntyp  |   | (Pure, 0 side effects)|   | .killed_leader        |
+-----------+-----------+   +-----------+-----------+   +-----------+-----------+
            \                           |                           /
             \                          v                          /
              +-----------------> netrust-sim <-------------------+
                                  - World creation: init alignment from role
                                  - Monster spawn: set actor.malign
                                  - Combat/Kill: on_actor_killed calls adjalign
```

---

## 3. Detailed Specifications

### 3.1 Per-Role Initial Alignment Record
In `netrust-data`:
- Add `initial_alignment_record: i32` to `RoleSpec`:
  - `RoleId::Valkyrie` -> 0
  - `RoleId::Wizard` -> 0
  - `RoleId::Barbarian` -> 10
  - `RoleId::Rogue` -> 10
  - `RoleId::Knight` -> 10
  - `RoleId::Monk` -> 10
  - `RoleId::Healer` -> 10
  - `RoleId::Tourist` -> 0
  - `RoleId::Archaeologist` -> 10
- In `RoleDef`:
  - `#[serde(default = "default_initial_alignment_record")] pub initial_alignment_record: i32`
  - Helper `fn default_initial_alignment_record() -> i32 { 10 }`
  - In `vanilla_ruleset()`, set from the role's canonical `initrecord`.
- In `netrust-sim`:
  - `world.alignment_record` initializes to `ruleset.role(config.role).map(|r| r.initial_alignment_record).unwrap_or(INITIAL_ALIGNMENT_RECORD)`.

### 3.2 Monster Malign Calculation (`netrust-core`)
Add pure function in `crates/netrust-core/src/peace.rs` (or `crates/netrust-core/src/alignment.rs`):
```rust
/// Compute C `malign` (makemon.c:2320-2366) upon monster creation.
///
/// Precalculated alignment adjustment upon death. Negative indicates bad to kill;
/// positive indicates righteous kill.
pub fn calculate_malign(
    maligntyp: i8,
    hero_alignment: Alignment,
    is_peaceful: bool,
    msound: MonsterSound,
    always_peaceful: bool,
    always_hostile: bool,
) -> i32
```
Rules (verbatim C `makemon.c:2338-2366`):
1. `let mal = maligntyp as i32;`
2. `let coaligned = sgn(mal) == sgn(hero_alignment);` where:
   - `sgn(Alignment::Chaotic) == -1`
   - `sgn(Alignment::Neutral) == 0`
   - `sgn(Alignment::Lawful) == 1`
   - `sgn(x) == -1 if x < 0, 0 if x == 0, 1 if x > 0`
3. If `msound == MonsterSound::Leader`:
   `malign = -20;`
4. Else if `maligntyp == -128` (`A_NONE`):
   `malign = if is_peaceful { 0 } else { 20 };`
5. Else if `always_peaceful`:
   `let absmal = mal.abs();`
   `malign = if is_peaceful { -3 * 5.max(absmal) } else { 3 * 5.max(absmal) };`
6. Else if `always_hostile`:
   `let absmal = mal.abs();`
   `malign = if coaligned { 0 } else { 5.max(absmal) };`
7. Else if `coaligned`:
   `let absmal = mal.abs();`
   `malign = if is_peaceful { -3 * 3.max(absmal) } else { 3.max(absmal) };`
8. Else (not coaligned and therefore hostile):
   `malign = mal.abs();`

### 3.3 ActorRecord Storage & Serde (`netrust-arena`)
- In `ActorRecord`:
  ```rust
  /// C `malign` (`makemon.c:2320-2366`). Precalculated alignment adjustment upon death.
  #[serde(default)]
  pub malign: i32,
  ```
- Serde default test verifying older saves without `malign` load safely with `malign == 0`.

### 3.4 Kill-Based Alignment Adjustments (`netrust-sim`)
In `crates/netrust-sim/src/combat.rs`:
When a monster is killed by the hero (`attacker_id == self.player_id`):
```rust
let lim = netrust_core::alignlim(self.scheduler.turn);
let quest_cfg = netrust_core::get_role_quest_config_or_default(&self.role_name);
let is_leader = defender.name.eq_ignore_ascii_case(quest_cfg.leader_name);
let is_nemesis = defender.name.eq_ignore_ascii_case(quest_cfg.nemesis_name);
let is_guardian = defender.name.eq_ignore_ascii_case(quest_cfg.guardian_name);
let is_priest = defender.name.to_lowercase().contains("priest");

if is_leader {
    // REAL BAD! mon.c:3678
    let penalty = -(self.alignment_record + lim / 2);
    self.alignment_record = netrust_core::adjalign(self.alignment_record, penalty, lim);
    self.quest_state.killed_leader = true;
    self.god_anger = self.god_anger.saturating_add(7);
    self.hero.luck = self.hero.luck.saturating_sub(20);
    // anger quest guardians on the level
    self.anger_quest_guardians();
} else if is_nemesis {
    // Real good! mon.c:3685
    if !self.quest_state.killed_leader {
        self.alignment_record = netrust_core::adjalign(self.alignment_record, lim / 4, lim);
    }
} else if is_guardian {
    // Bad mon.c:3689
    self.alignment_record = netrust_core::adjalign(self.alignment_record, -(lim / 8), lim);
    self.god_anger = self.god_anger.saturating_add(1);
    self.hero.luck = self.hero.luck.saturating_sub(4);
} else if is_priest {
    let coaligned = defender.alignment == self.hero_alignment;
    let n = if coaligned { -2 } else { 2 };
    self.alignment_record = netrust_core::adjalign(self.alignment_record, n, lim);
    if coaligned {
        self.divine_protection = 0;
    }
    // High priest of Moloch
    if defender_maligntyp == -128 {
        self.alignment_record = netrust_core::adjalign(self.alignment_record, lim / 4, lim);
    }
} else if defender.is_tame {
    self.alignment_record = netrust_core::adjalign(self.alignment_record, -15, lim);
} else if defender.is_peaceful {
    self.alignment_record = netrust_core::adjalign(self.alignment_record, -5, lim);
}

// Unconditionally apply precalculated malign (mon.c:3725)
self.alignment_record = netrust_core::adjalign(self.alignment_record, defender.malign, lim);
```

---

## 4. Invariants & Verification Strategy

1. **Pure Reference Model Proptests**:
   Compare `calculate_malign` against a reference implementation directly transcribing C `makemon.c:2320-2366` for all random input tuples `(maligntyp, hero_align, is_peaceful, msound, always_peaceful, always_hostile)`.
2. **Serde Backward Compatibility**:
   Save files missing `malign` on `ActorRecord` or `killed_leader` on `QuestState` deserialize with defaults.
3. **Determinism & Baseline Fingerprints**:
   Update `crates/netrust-agent/tests/golden_determinism.rs` baselines since initial alignment and monster malign kill updates alter game progression deterministically.

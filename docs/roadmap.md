# NetHackED Roadmap

*NetHackED — from Yemelianov (Emelyanov Dmytro)*

## Where NetHackED stands

NetHackED is a partial reimplementation of NetHack 5.0 in Rust. The [parity tables](parity/README.md) compare it with the C source feature by feature, with evidence for every row. At the 2026-10-05 baseline, 542 player-visible features were audited:

| ✅ Present | 🟡 Partial | ❌ Missing | ❔ Unsure |
|---:|---:|---:|---:|
| 58 (11%) | 175 (32%) | 306 (56%) | 3 |

Content coverage at the same baseline:
- 55 of 394 monster species;
- 51 of 439 object types (0 of 28 rings);
- 9 of 13 roles;
- 85 commands in C, of which 7 work as in C.

What already works well:
- a deterministic engine with save-compatible state and replayable seeds;
- C-faithful melee dice, peacefulness and alignment rules;
- the invocation endgame;
- rule packs that are byte-identical between the CLI and the browser;
- browser, terminal, MCP, JSON-RPC and GraphQL frontends;
- Lean 4 models of selected mechanics. They are machine-checked but not formally linked to the Rust code.

The roadmap below orders the remaining work by what a player notices first. Each milestone is defined by the parity-table rows it must turn green.

## Rules this roadmap follows

1. **The tables are the source of truth.** A feature is ✅ only when its row says so, with Rust evidence and a test that encodes the C behavior. The README and release notes never claim more than the tables.
2. **Every change cites C.** Changed Rust functions cite `file.c:line` in the NetHack 5.0 tree. Where the engine deliberately differs, the gap goes into "Known divergences" in [formal-mechanics-spec.md](formal-mechanics-spec.md) and the row stays 🟡.
3. **Determinism is kept on purpose.** The golden determinism fingerprints change only when a milestone changes mechanics, such as new RNG draws. That change is stated in the pull request and the release notes.
4. **Old saves and packs keep loading.** New state fields use `#[serde(default)]` with a load test, and renamed formats keep a reader for the old one.
5. **Each milestone is a release.** It ships as a minor version. The release notes give the before and after counts from `scripts/parity-summary.py` and list the rows that changed.
6. **Status is updated in the same PR.** A PR that changes behavior updates its rows and the summary, and CI checks that the summary matches the tables.

## Done

| Release | What it delivered |
|---|---|
| v0.1.0 | Engine, frontends and Lean 4 models baseline |
| v0.2.0 | Fidelity D1–D3: C data tables for monsters, items and pantheons; C monster attack resolution; `peace_minded`; per-role alignment records; `set_malign`; kill-based alignment and god anger. Rule packs P1. |
| v0.3.0 | Renamed to NetHackED; browser terminal at [nethacked.yemelianov.dev](https://nethacked.yemelianov.dev/); rule pack manager and editor; `install`/`list`/`info` |
| (main) | Full Ukrainian UI in the browser terminal; pixel renderer from [pixel-ssh](https://github.com/dmytro-yemelianov/pixel-ssh) with real Cyrillic glyphs; this roadmap and the parity tables |

## Milestones

Size is a rough guide to effort: **S** is days, **M** is about a week, **L** is a few weeks, **XL** is longer. Each milestone gets a spec and a plan in `docs/superpowers/` before work starts.

### M1 — Playable core loop · M · next

**Goal.** The game reads and controls like NetHack.

**Scope:**
- map memory, so explored tiles stay drawn;
- item prompts in the terminal (`What do you want to eat? [a-c or ?*]`), the same as the browser;
- look commands `;` `:` `/` and "You see here";
- message history `^P`, and every message shown, not just the last;
- monsters and objects drawn with their C glyphs, not the first letter of their name;
- doors blocking diagonal moves;
- the `z` command choosing which wand to zap.

**Exit criteria.** The matching rows in [flow-ui.md](parity/flow-ui.md) sections A and C, and the map memory and vision rows in [objects-dungeon.md](parity/objects-dungeon.md), are ✅. Both frontends behave the same way.

### M2 — Hero progression and endings · L

**Goal.** The hero grows, and a game ends the way NetHack's does.

**Scope:**
- experience points and level gain or loss with HP and Pw growth (`exper.c`);
- natural HP and Pw regeneration;
- attributes St/Dx/Co/In/Wi/Ch with Str bonuses to hit and damage and a carry-weight limit (`attrib.c`);
- the full status line: Xp level, attributes, encumbrance, and the conditions blind, confused, stunned and hallucinating;
- cause of death, `disclose()` (DYWYPI: possessions, attributes, vanquished, genocided, conduct, overview), the tombstone (`rip.c`), score and the high-score list (`topten.c`);
- ascension ending the game;
- save and restore.

**Exit criteria.** [hero.md](parity/hero.md) "Experience" and "Attributes" rows are ✅, and [flow-ui.md](parity/flow-ui.md) section B is ✅. Golden fingerprints are updated on purpose, because level-up adds C's HP dice rolls.

### M3 — Equipment · L

**Goal.** What you carry is not what you wear.

**Scope:**
- wear, take off, put on and remove;
- rings and amulets added to the catalog, with their properties: magic resistance, reflection, free action and others;
- item properties granting intrinsics;
- encumbrance;
- a throw command with launchers and multishot (`dothrow.c`).

**Exit criteria.** [hero.md](parity/hero.md) "Armor and accessories" and "Wielding" rows are ✅. Ring and amulet coverage reaches C's 28 and 13.

### M4 — Identification and items · L

**Goal.** Bring back the identification game.

**Scope:**
- shuffled appearances and per-game knowledge (`o_init.c`);
- the discoveries list `` ` ``;
- names with BUC, enchantment, erosion and charges, plus stacks and quantities (`objnam.c`);
- random loot with BUC and enchantment rolls (`mkobj.c`);
- container contents;
- a wishing parser with C's enchantment cap and quantities.

**Exit criteria.** The identification, naming and generation rows in [objects-dungeon.md](parity/objects-dungeon.md) are ✅.

### M5 — Living monsters · XL

**Goal.** Monsters act like NetHack monsters.

**Scope:**
- monster speed and regeneration;
- sleeping and waking monsters;
- monsters opening doors;
- monster inventory and wielded weapons, and monsters using items (`muse.c`);
- side effects for every `AD_*` damage type (poison, level drain, stoning, paralysis, theft, erosion, engulfing, exploding);
- generation by difficulty and ongoing spawns (`makemon.c`);
- the starting pet; pets taming, following on stairs, eating and fetching;
- shops with selling, price identification, Keystone Kops and usage fees;
- vault guards;
- the Wizard of Yendor and his harassment.

**Exit criteria.** The [monsters-combat.md](parity/monsters-combat.md) sections makemon, monmove, mhitu, muse, dog, shk, vault and wizard are ✅.

### M6 — Dungeon breadth · XL

**Goal.** The full NetHack dungeon.

**Scope:**
- C level generation: doors, secret doors and corridors; special rooms (zoos, barracks, beehives, throne rooms, temples with priests, typed shops); fountains, sinks and thrones;
- all trap types in normal play, with their effects;
- the full dungeon structure from `dungeon.lua`: Oracle, Big Room, the real Sokoban (4 levels, 8 maps), all Minetown and Mines' End variants, a separate quest per role, Fort Ludios, Medusa, Castle, Valley, the Gehennom lairs, the towers, and the Elemental and Astral Planes.

**Exit criteria.** The level, trap and branch rows in [objects-dungeon.md](parity/objects-dungeon.md) are ✅. Level definitions come from the C `dat/*.lua` maps.

### M7 — Content breadth · XL

**Goal.** C's full catalogs and the remaining hero systems.

**Scope:**
- all 394 monster species and all 439 object types;
- all 13 roles, with race, gender and alignment choice and the full starting kits;
- spells with C failure rates and memory;
- weapon skills with practice and `#enhance`;
- prayer fixing troubles;
- corpse effects and intrinsics;
- polymorph and lycanthropy;
- the timed afflictions (stoning, sliming, sickness, strangulation, levitation);
- riding with a saddle;
- artifacts with their targets and `#invoke`.

**Exit criteria.** The content coverage table in [parity/README.md](parity/README.md) shows 100% for monsters, objects and roles, and the remaining [hero.md](parity/hero.md) sections are ✅.

### M8 — Text, flavor and meta · M

**Goal.** The parts that make NetHack NetHack.

**Scope:**
- rumors and fortune cookies;
- Oracle consultations;
- the full quest text (`questpgr.c`);
- moon-phase and Friday-13th luck (`calendar.c`);
- `#overview` and enlightenment;
- conduct and achievement display;
- options;
- bones on every frontend.

**Exit criteria.** [flow-ui.md](parity/flow-ui.md) sections D and F are ✅.

## Order and dependencies

```
M1 core loop ──► M2 progression & endings ──► M3 equipment ──► M4 identification
                                    │                                  │
                                    └──────────► M5 living monsters ◄──┘
                                                       │
                                     M6 dungeon breadth ◄┘ ──► M7 content breadth ──► M8 flavor
```

- M2 depends on M1's message and status plumbing.
- M3 needs M2's attributes for encumbrance and to-hit.
- M4 builds on M3's item properties.
- M5's monster inventory reuses M4's object generation.
- M6 levels need M5's generation by difficulty.
- M7 widens every catalog that M3–M6 make work.

Small fixes can land in any order.

## Out of scope for now

- Multiplayer and tournament servers, beyond the existing agent APIs.
- Interface options with no gameplay effect, such as window ports and curses layout options.
- Formal links between the Lean models and the Rust code. The Lean models stay as specifications and a basis for property tests.

## Changing this roadmap

Reorder milestones only through a pull request that edits this file and states why. Re-run the parity audit before each milestone release, so the tables catch regressions as well as progress.

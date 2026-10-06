# Item initialization: bounded delivery contract

Baseline: `e6bc619`, `feat/monster-hp-d8`. Execute I1, independent review, full gates and commit; then I2 and I3 as separate acceptance units. Coordinator owns the P1 progress ledger and authoritative RNG/golden policy. The user explicitly approved updating affected goldens for verified intentional item/RNG changes after independent review; this exception is scoped to item initialization. Follow [execution loops](2026-10-06-agentic-loops.md). No new dependencies, C runtime calls, duplicate handwritten C tables or weakened assertions.

User-requested scope is random BUC, enchantment/charges, container contents and random artifacts. This is not complete `mksobj` fidelity: monster payloads, timers, food variants, erosion/grease, oil/fuel, knowledge flags and role/context extras also exist in C. Keep those divergences explicit.

## Shared integration and data contract

Sources: vendored `src/mkobj.c`: `mkobj` 270, `mkbox_cnts` 304, `mksobj_init` 869, `mksobj` 1179, `blessorcurse` 1841; `src/rnd.c:rne` 192; `src/artifact.c:mk_artifact` 172 and `nartifact_exist` 462. References below are relative to `NetHack-5.0.0/`.

Current integration: `nethacked-data::generation::mkobj_kind` selects from static catalog probabilities; ordinary-room generation in `nethacked-sim/src/actions/stairs.rs` around 790 then calls owned `Ruleset::create_item_record_by_id`, which supplies uncursed/catalog defaults. `ItemRecord` stores C `spe` in `enchantment: i8`; `recharged` already exists. `ItemLocation::InContainer` and arena containment exist. Explicit factories serve starting inventory, scripted prizes, corpses/bones and artifacts; preserve their supplied state and RNG-free semantics. Add a distinct random-generation API and route ordinary-room generation through it after kind selection, before the continuation draw. Define failed-placement behavior explicitly and test it; do not silently discard initializer draws for an object C would create.

Use owned Ruleset definitions for initialization and recursive selection. Add generated initialization metadata from extractor output: charged flag, weapon skill predicates and relevant C special tags; never classify by translated/display names. Existing `ItemDef.id` allows vanilla dispatch. Custom items default to fixed initialization; add explicit opt-in profile, validated against class. Built-in identity plus owned override determines behavior; preserve overridden name/weight/direction. Do not introduce unrelated custom-item selection changes: current static kind selection cannot select custom items. New fields need serde defaults, old-save/old-pack load coverage and canonical schema/hash checks.

## I1 — BUC and `spe`, with a truthful supported subset

Add pure core branch helpers using explicit named/clamped rolls, and an owned-data sampler accepting `rn2(n)` and hero level. A scripted callback must prove exact draw bounds/order. `rnd(n)=rn2(n)+1`, `rn1(n,b)=rn2(n)+b`. `rne(3)` begins at 1, repeats `rn2(3)==0` while below cap: cap 5 below hero level 15, otherwise level/3. Reaching the cap consumes no extra draw.

`blessorcurse(chance)` consumes nothing for already blessed/cursed objects; otherwise `rn2(chance)`, then only on zero `rn2(2)` (zero cursed, one blessed). Start fresh random objects uncursed with `spe=0`, not the deterministic factory's midrange wand charges.

Supported branch order:

* Weapons: quantity first (`rn1(6,6)` for C multigen, otherwise 1); `rn2(11)==0`: positive `rne(3)`, then blessed=`rn2(2)`; otherwise `rn2(10)==0`: curse then negative `rne(3)`; otherwise BC(10). Poisonable: then `rn2(100)==0`. Add quantity default 1 and poisoned default false so these preceding draws have actual state. Artifact hook follows in I3.
* Armor: first `rn2(10)`; if nonzero and a harmful kind (fumble/levitation boots, opposite-alignment helm, fumbling gauntlets), or otherwise the conditional `rn2(11)==0`, curse then negative `rne(3)`. If that branch fails: `rn2(10)==0` gives blessed=`rn2(2)` **before** positive `rne(3)`; else BC(10). Artifact hook follows. Samurai lacquer remains documented.
* Wands: wishing=1 without charge draw; stasis=`rn1(4,3)`; other NODIR=`rn1(5,11)`, directional=`rn1(5,4)`; then BC(17), recharged=0.
* Charged rings: BC(3); outer `rn2(10)` nonzero then inner `rn2(10)` (even when uncursed); inner nonzero and BUC sign nonzero uses sign times `rne(3)`, otherwise sign via `rn2(2)` then `rne(3)`. If zero `spe`, `rn2(4)-rn2(3)`; negative `spe` then `rn2(5)` nonzero curses. Uncharged rings: `rn2(10)` nonzero then harmful-kind test (teleportation/polymorph/aggravation/hunger), or conditional `rn2(9)==0`, curses.
* Amulets: always first `rn2(10)`; nonzero harmful strangulation/change/restful-sleep kind curses; otherwise BC(10). Potions/scrolls BC(4); books BC(17). Mail conditional depends on the selected C build configuration.
* Simple tools: camera/tinning kit/marker `rn1(70,30)`; grease `rn1(21,5)` then BC(10); crystal ball `rn1(5,3)` then BC(2); plenty horn/tricks bag `rn1(18,3)`; magical instruments `rn1(5,4)`; Bell of Opening=3; magic lamp=1 then BC(2). Loadstone forced cursed.

I1 deliberately excludes candle/lamp fuel and quantity, tins/figurines (their species draws precede BUC), and all other food/gem payload initialization. Never synthesize skipped draws or claim an exact full-C stream. Containers remain empty until I2; I1's public name/documentation must describe the supported fields/subset. Erosion tail and permanent poison remain separate gaps.

Tests: every branch and short circuit, `rne` levels 14/15/18 and cap exhaustion, charge endpoints/stasis, ring zero fallback/signs, preexisting BUC, custom fixed/opt-in and renamed overrides; C-reference property tests; seeded real room integration and replay round trip. Existing charge assertions must encode C ranges without weakening unrelated assertions.

## I2 — represented container contents

Add locked/trapped/trap-known defaults false; chest/large box draw `rn2(5)!=0`, `rn2(10)==0`, then only when trapped `rn2(100)==0`. Capacity maximum: chest locked 7/unlocked 5, box 5/3; sack/oilskin maximum 0 for moves<=1 outside level generation, otherwise 1; holding bag 1. Draw `rn2(max+1)` even at max=0. Generate children in loop order: Box class draw, weighted kind draw, I1 initialization with artifacts disabled, then corrections, then attach.

Coin quantity=`rnd(difficulty+2)*rnd(75)`. Rocks repeatedly select C weighted `rnd_class(DILITHIUM_CRYSTAL,LOADSTONE)` until non-rock; quantity>2 becomes 1. Holding bags convert magical bags to sack with spe=0, or repeatedly replace cancellation wand through weighted `rnd_class(WAN_LIGHT,WAN_LIGHTNING)`, preserving other initialized state. Add owned generation weights/range selection; generated data already carries Box probabilities. Recompute contained weights and honor holding-bag BUC semantics.

Ice boxes require corpse species selection and timer-preservation support (`age=0`). Deliver I2a for the five ordinary container kinds, then I2b for represented corpse payload selection and preserved ice-box contents before I3. Missing revival/glob timer systems remain explicit divergences; do not emulate unavailable timers or claim full corpse fidelity. Test nested locations, draw trace, boundaries, substitutions, weight, moves/mklev context, save/load and deterministic recursive generation.

## I3 — random existing-object artifacts

Generate an owned artifact metadata table from existing `artifacts.toml` (base, nogen, gen_spe, gift_value, role, C order); expose object unique flag. Add stable artifact identity and world existence tracking, including scripted/quest/bones/imported artifacts; uniqueness survives destruction and saves. Weapon hook draws `rn2(20+10*existing)`; armor `rn2(40+10*existing)`, only with artifact-enabled context, at the positions above.

On zero, C `mk_artifact(obj,A_NONE,99,TRUE)` filters existing/nogen/unique-base artifacts, gift_value>99 unless hero role matches, then matching base; preserves C order. No candidate means no selection draw; otherwise `rn2(candidate_count)` even for one. Set identity/name/existence, reset erosion, add gen_spe only if result is >=-10 and <10, apply permanent poison. No divine-gift alignment/skill draws belong here. Test probability bounds, ordered candidates, exhausted pool, role exemption, spe limits, reload/destruction uniqueness and real room hook. Keep divine gifts, naming/wishing, role-specific `hack_artifacts` adjustments and artifact powers explicit until their contexts are implemented.

Each unit requires independent review and the complete execution-loop gate. New room RNG consumption likely changes later level/game fingerprints; compare all three goldens and seek coordinator policy resolution before any baseline edit. HP approval does not authorize item rebaselining.

## I1 acceptance — 2026-10-06

I1 accepted after independent Sol/high review and one targeted test-gap repair. Fresh final gate passed: 715 workspace tests, all three goldens, formatting, Clippy, WASM/Python, Lean, static/data-generation/coverage/parity and docs checks. Three `EXPECTED_RUNS` entries were checked and rebaselined under explicit user policy; `EXPECTED_LOGS` and `EXPECTED_COMBAT` stayed unchanged. The closing audit found run constants already modified (not attributed to the owned implementer); root saved them, restored committed constants and proved the actual 2-pass/1-fail comparison before accepting matching values. The premature no-rebaseline update was corrected. No dependency changes.

Regressions prove short-circuit draws, old record/profile/hash defaults, real room/save continuation and rejected-placement draw consumption/order. Logs: `/tmp/netrust-item-i1-focused.log`, `/tmp/netrust-item-i1-repair1.log`, `/tmp/netrust-item-i1-gates/golden-committed-comparison.log`, `/tmp/netrust-item-i1-final-gates/`. Model usage unavailable; I2a/I2b/I3 remain next.

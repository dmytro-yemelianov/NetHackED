# Scope all seven remaining full-parity streams

Planning only at `2f97135ec4c5620ad132bfe0971bf3aff60ff7bf`,342-input digest `a3135b6d4348eaf56c89bc2f0d11f642993f5fc6c94ea2a749097286a213981c`. Reuse the accepted 999-test/all 3-golden/all 12-gate checkpoint; no fresh 543-row semantic audit, implementation or C execution. Source authority: released NetHack 5.0 commit `16ff59115315917b93185d026aeefea06db9b0f4` in `/Users/dmytro/github/NetHack` via `git show`.

The target is gameplay/content/player-command semantics under a source-contract-pinned C configuration. Exact C RNG replay, original window-port presentation and binary C save/bones interchange remain separately optional; Rust save continuation and intended frontend reachability are mandatory. No completion percentage, ETA or remaining validated-task count is inferred from this plan.

## Registry and contract rule

[obligation registry](../parity/obligations/2026-10-07-remaining-parity-scope.json) preserves all 543 inventory rows by path+ordinal, historic status and C reference; each points to a family and a ≤20-row reconciliation packet. It contains 53 family cards, 53 inventory packets and 1023 named catalog coverage instances including command/tag expansions (1129 planning cards total). These 1,023 catalog instances are coverage obligations/cases, not 1,023 implementation tasks or commits. Counts describe planning structure, not estimated remaining work. Shared AT/AD/flag handlers own common behavior; 383 species instances do not require 383 separate implementations. Present rows require regression verification; historic missing/partial/unsure rows require audit before any current gap claim. The family routing is based on section/source/content. Source audits may reassign intersecting rows with evidence.

[obligation registry](../parity/obligations/2026-10-07-remaining-parity-scope.json) stores shared status, contract gate, exits, routing, repair limits, evidence and verification in `contract_defaults`. An effective card is the shallow merge of those defaults with the card, whose explicit values take precedence. `kind` is `family`, `inventory_packet` or `catalog_instance`; instances trace coverage cases rather than commit-sized assignments. Every effective card currently has `planned_audit_needed` status. Its first exit is a reviewed source contract: exact pinned symbols/configuration/branches, independently derived expected fixtures, audited current owned Rust owner/public action, prerequisites, permitted files and exclusions mapped to successor IDs. A source-confirmed existing path earns evidence and regression coverage rather than a duplicate implementation. The owner candidates use known historical paths from accepted plans/records; graph results still expose `netrust-*` names, so these paths are audit candidates, not new verified wiring claims.

A broad family is never one giant dispatch. Choose one coherent branch/context/member, assign a stable `<family>-<branch>` or `CAT-<catalog>-<source-id>-<context>` child, and record dependencies before ready. Split timing, selection, effect and persistence at their natural ownership boundaries. Every unlisted C branch found during family audit becomes an explicit child or a reviewed out-of-scope configuration exclusion; no silent unsupported early slice.

## Prerequisite readiness and family closure

Family edges are routing and audit prerequisites. A catalog instance or inventory packet pointing to its parent requires the relevant reviewed contract or shared foundation, rather than completion of every member of that family. Before dispatch, bind exact accepted prerequisite child IDs, evidence hashes and the interface/invariant consumed. A narrow child may proceed while sibling branches remain open. The expanded child graph must pass its own dependency check.

Family/catalog closure is a separate aggregate: every scoped branch/member/context must be accepted, regression verified or excluded with reviewed source/configuration evidence. Member work contributes to that closure; closure cannot become the member's prerequisite.

DG03 must register narrow public Kick contracts for doors (`DG03-KICK-DOOR`), objects/containers (`DG03-KICK-OBJECT`), monsters (`DG03-KICK-MONSTER`) and terrain (`DG03-KICK-TERRAIN`). These successor names reserve audit work, not ready tasks. Include failed checks, hurt-self damage, selection/cancellation and turn costs; link historical command `CMD-0034` and rows `objects-dungeon:0055–0057`. Bind narrow effective-attribute, combat, object-ownership and terrain prerequisites from EQ01/CB01/OB01/DG01 as each audited context requires.

## Bounded family cards

| ID | Selected source family and branch scope | Dependencies |
|---|---|---|
| EQ01 Effective attributes | attrib.c acurr, acurrstr; worn.c property masks: gear/temporary STR DEX CON and clipping | source audit |
| EQ02 Wear/wield slots | do_wear.c; wield.c; worn.c: slot blockers, curses, delays, interruption, form disrobing | EQ01 |
| EQ03 Property provenance | prop.h; worn.c; artifact.c set_artifact_intrinsic: each source/slot, stacked removal, suppression; MC guarding/protection | EQ02 |
| EQ04 Capacity/stamina | hack.c weight_cap, inv_weight, calc_capacity: mount/form/legs/air; coins/loadstone, partial-stack prompts, low HP stamina/exercise | EQ01,OB01 |
| EQ05 Weapon modifiers | weapon.c; uhitm.c: twohand/dual/rings, launcher/ammo/multishot eligibility | EQ02,HE03 |
| P11a Explicit wand selection/charge | zap.c dozap, zappable; getobj paths: selected owned item, cancellation, empty/wrest/backfire, charge and turn order; preserve create monster | source audit |
| P11b Beam traversal foundation | zap.c buzz, bhit, zap_over_floor: range, walls/doors, bounce/self-hit, multi-target and ordered cells; terrain branch audit | P11a,DG01 |
| P11c Ray resistance/reflection | zap.c zhitm, zhitu; resist; worn.c: actor effective properties, MR vs resistance, reflection source, damage draw order | P11b,EQ03 |
| P11d Fire branch slice | zap.c burn_floor_paper, destroy_items; uhitm.c fire handlers: fire damage/slime cure, inventory/floor burning, armor, ice/terrain and secondary dragon effects; one context per contract | P11c,OB02,DG01 |
| CB01 Ordinary hit/damage terms | uhitm.c find_roll_to_hit, hmon; weapon.c dmgval: monster-state/luck/weapon/material/blessing/bare-hand terms | EQ05 |
| CB02 AT/AD context matrix | monattk.h; mhitu.c, uhitm.c, mhitm.c: each attack/damage pair in hero->monster, monster->hero, monster->monster; MC, resistances, fatal early returns | CB01,EQ03 |
| CB03 Passives/engulf/gaze/explosion | mhitu.c gazemu,gulpmu,explmu,passiveum; mhitm.c passivemm,gulpmm; explode.c: context transitions, visibility, reflected gaze, drowning/digestion, passive ordering | CB02,P11c |
| CB04 Projectile/breath/spells | dothrow.c; mthrowu.c; mcastu.c: range, hit, break/return, multishot; breath ray reuse; mage/cleric lists | P11c,EQ05,OB01 |
| CB05 Infighting/polymorph combat | mhitm.c fightm; uhitm.c hmonas: hostile pet targets and full form attacks with passive outcome propagation | CB03,HE07,AI04 |
| AI01 Spawn contexts/groups/HP | makemon.c rndmonst,mkclass,adj_lev,m_initgrp; allmain.c: LGROUP/other groups, ongoing cadence, habitat/planes, genocide/difficulty, mavenge, special creation | source audit |
| AI02 Movement/senses/scheduling | monmove.c m_move,distfleeck,disturb; mon.c mcalcmove: sleep/wait, fleeing/memory, senses/displacement, doors/tunnel/swim/fly; MSLOW/MFAST, linked-head ordering; death credit/same-pass spawn tests | AI01,DG01,EQ03 |
| AI03 Monster inventory/item use | makemon.c m_initweap,m_initinv; mon.c mpickstuff; muse.c; steal.c relobj: weapon/loot kits, pickup/equip/drop; defensive/offensive/misc item decisions | AI01,OB01,OB02 |
| AI04 Pet creation/taming/continuation | dog.c makedog,tamedog,keepdogs,losedogs; dogmove.c: starting pet, food taming, migration, starvation/apport, goals, abuse/untaming, grow_up/promotion | AI02,OB03,HE01 |
| AI05 Special monster lifecycle | mon.c newcham,monstone,mon_regen; worm.c; minion.c; mplayer.c: regen/growth, lifesaving/vampires/stoning, worm segments, summons/minions/player monsters; global identifier wrap | AI02,CB02,OB04 |
| OB01 Ownership/stacks/knowledge/naming | invent.c merged; objnam.c; o_init.c; pickup.c: split/merge, hidden known/bknown/dknown, shuffled appearances/discoveries, naming quantities/articles, container transactions | source audit |
| OB02 Object init/erosion/destruction | mkobj.c mksobj; trap.c erode_obj; zap.c destroy_items: unmodeled BUC/spe/payload/erosion/fuel, statues/eggs/tins/globs, quantity/weight, destruction ownership | OB01,AI01 |
| OB03 Eating/corpse effects | eat.c bite,cprefx,cpostfx,givit,gethungry: meal occupation/interruption, all food/species nutrition/intrinsics/poison/cannibal/choke/faint/hunger sources | OB01,HE02,EQ03 |
| OB04 Timers/corpse lifecycle | timeout.c; mkobj.c; zap.c revive; mon.c make_corpse: Rider/Ice execution, buried contents, mextra/shop/quest/special identities; hatching/fuel/figurines/globs; active/cache/icebox and id wrap | OB02,AI01 |
| OB05 Potion effect contexts | potion.c peffects,potionhit,potionbreathe,mixtype: each potion BUC, vapor/hit/dip/dilution/alchemy and identification observation | OB03,CB04 |
| OB06 Scroll effect contexts | read.c seffects: each scroll BUC/confused/blind; consumptions, prompts, cancellation, destruction | OB01,EQ03,DG02 |
| OB07 Wands beyond fire | zap.c bhitm,bhito,zap_dig; apply.c do_break_wand: each wand immediate/nondirectional/ray/self/engulf/object pile; digging/breaking; create-monster variants | P11d,CB03,AI01,DG03 |
| OB08 Equipment class effects | do_wear.c; worn.c; ball.c: all armor/rings/amulets/weapons/gems/stones, punishment ball/chain, unicorn gifts | EQ03,OB01 |
| OB09 Tool apply/rub/write/locks | apply.c; lock.c; write.c; dig.c: every tool, fuel/wish, mirror/stethoscope/unihorn/towel/leash/saddle/tin/figurine/grease/camera/crystal-ball, marker and locks | OB04,DG03,AI04 |
| OB10 Artifacts/wishing | artifact.c; objnam.c readobjnam; pray.c; fountain.c: all artifact hit/target bonuses, Magicbane/behead/touch/blast, carried/invoked/light/speak, gifts/history; complete wish parser/restrictions | CB02,EQ03,HE05 |
| OB11 Containers/polypiling | pickup.c in_container,out_container,doloot,dotip; zap.c poly_obj: BoH scatter/loss, all contents contexts, cursed cases, pile transformations | OB01,OB02,OB07 |
| HE01 Character selection/kits | role.c validrole/race/gend/align; u_init.c ini_inv: all 13 roles, 5 races and valid gender/alignment combinations; kits/known objects/skills/pets | source audit |
| HE02 XP/HP/Pw/attributes | exper.c; attrib.c init_attr,adjattrib,adjabil,newhp: XP discounts/eel, rank titles, Wis energy, recorded gains/losses, intrinsics, exercise/abuse/restoration | HE01 |
| HE03 Skills | weapon.c skill tables,enhance_weapon_skill: starting caps, practice, advancement, restrictions and drain | HE02 |
| HE04 HP/Pw regeneration/properties | allmain.c regen_hp,regen_pw: Pw, Regeneration/Sleepy/prayer, polymorph and effective CON; once per world tick | HE02,EQ03 |
| HE05 Prayer/sacrifice | pray.c dosacrifice,dopray,in_trouble,fix_worst_trouble: trouble selection, anger/luck/alignment/time, altar conversion, gifts/crowning, corpse cases | HE02,OB03 |
| HE06 Spells/books | spell.c study_book,percent_success,spelleffects; read.c: all books/spells, study/memory/amnesia, failure/Pw/hunger/skill, target/prompt cancellation | HE03,EQ03 |
| HE07 Polymorph/lycanthropy | polyself.c; were.c: form properties/HP, armor break/drop, controls, monster attacks/summon/cure and rehumanization | EQ03,HE04 |
| HE08 Timed hero conditions | timeout.c nh_timeout/dialogues: stoning/sliming/strangle/sick/vomit/levitation/legs/hallucination/sleep; dialogue/fatal priorities | HE04,CB02 |
| HE09 Riding | steed.c: saddle eligibility/failure, falls/kick/skill, gallop/mount burden/landing | HE03,AI04,EQ04 |
| HE10 Music/sitting/engraving | music.c; sit.c; engrave.c: instruments/tunes/earthquake, all seat/throne/egg/curse branches, media/tools/time, erasure/garbling/scare contexts | HE07,OB09,DG01 |
| DG01 Terrain/ordinary generation | mklev.c; mkroom.c; hack.c; fountain.c; dbridge.c: doors/secret corridors/rooms/vault/niche/themed terrain, bars/trees/graves/fountains/sinks/thrones/water/lava; drawbridges | source audit |
| DG02 Vision/light/detection/memory | vision.c; light.c; detect.c; display.c; rm.h: lit rooms/unlimited LOS, floor/monster sources, seenv/remembered glyphs, all detection contexts | DG01,OB01 |
| DG03 Traps/dig/teleport/regions | trap.c; teleport.c; dig.c; region.c; dokick.c: traps, escape/search, teleport, gas/regions; public Kick doors, objects/containers, monsters/terrain, failed checks and hurt-self damage | DG01,EQ03 |
| DG04 Topology/map engine | dungeon.c; dat/dungeon.lua; sp_lev.c des API; mkmap/mkmaze/extralev: data conversion without runtime Lua/C, branch depth constraints, fillers/mazes/Rogue, complete level DSL semantics | DG01,DG03,AI01 |
| DG05 Special maps/variants | dat/*.lua; sp_lev.c: all selected 124 level definitions, all variant placements/population/prizes/flags; no synthetic replacement closure | DG04,DG02,AI03 |
| DG06 NPC economy/roles | shk.c/shknam.c; priest.c; vault.c; sounds.c; rumors.c: shop types/bills/theft/anger/repair/price ID, priests/temples/guards/Oracle consultations | DG01,AI03,OB01 |
| DG07 Role quests | quest.c; questpgr.c; dat/<Role>-*.lua; quest.lua: all role home/locate/goal/fillers, leader/guardian/nemesis, eligibility/expulsion/artifact-return and text | HE01,DG05,DG06,OB10 |
| DG08 Wizard/endgame geography | wizard.c; covetous paths; dat planes/towers/lairs; do.c invocation: Wizard stealing/resurrection/harassment, Vlad/towers/demons/Riders, Planes/bubbles/Astral, ritual proper locations | DG05,DG07,CB03,AI05 |
| UI01 Command contract/dispatch | cmd.c extcmdlist and rhack: 85 catalog commands, aliases/count/repeat/menu/input/cancel/occupation/prompt semantics through intended frontends | source audit |
| UI02 Inventory/status/look/map output | invent.c; botl.c; pager.c; do_name.c; display.c: letters/sort/select/quantity, hidden knowledge/price-ID, naming, status/enlightenment/overview/map memory/messages | UI01,OB01,DG02 |
| UI03 Save/death/endings/scoring | save.c; restore.c; end.c; rip.c; topten.c; allmain.c: Rust continuation/disclosure/grave/death/lifesaving/escape/ascension/conduct/score; compressed endgame remains partial | UI01,DG08 |
| UI04 Text/calendar/conduct/options | insight.c; rumors.c; questpgr.c; calendar.c; options.c; mail.c: quest/rumor/Oracle content, moon/Friday13 external inputs, journals/achievements/conduct, gameplay options/config branches | UI02,DG06,HE05 |
| UI05 Bones/public persistence | bones.c: eligibility, ownership/curse/ghost uniqueness/artifact/species knowledge, local/frontend restoration, network policy separately | UI03,OB04,AI05 |

## Finite catalog expansion

Objects use every named kind (`id` plus nonempty `name`) in `data/nethack-5.0/objects.toml` (440);23 unnamed appearance metadata entries remain explicitly mapped to OB01/OB02, grouped by class into equipment, food, potion, scroll, book, wand, tool and remaining coin/gem/rock/ball/chain/venom contracts. Registry members are actual IDs/names; unnamed randomized appearances are covered separately by OB01. Each member gets its action contexts, BUC/confusion/blindness/target branches, identification/consumption/turn ordering and cross-class interactions. A ten-wand catchall is not a valid implementation assignment.

Artifacts use all 33 IDs in `artifacts.toml`; each carries hit/touch/carry/invoke/gift/light/speak contexts and creation-history uniqueness where applicable. Roles use all 13 IDs in `roles.toml`; races all 5 in `races.toml`; cross-product validity/gender/alignment comes from pinned `role.c` masks, then per-role kit/spells/skills/pet and per-role quest contracts. Monsters use all 383 IDs in `monsters.toml` for owned generation/lifecycle catalog coverage; AT/AD/flag behavior is an additional matrix, not383 copied handlers.

Commands cover the 85 historic flow-ui A entries by stable row key/name. Audit pinned `cmd.c extcmdlist[]`, aliases and config/debug branches before closing this catalog. Each card follows the public native action through intended browser/terminal/MCP/JSON-RPC/GraphQL/WASM/agent/Python surfaces, with a declared applicability matrix and tested prompt/cancel/repeat/time semantics. Core helper coverage alone cannot close a command.

Mandatory extraction inputs remain explicit: all AT_*/AD_* definitions including aliases/sentinels (registry additionally enumerates 17 used AT and 47 used AD tags, without claiming these exhaust declared behavior) in pinned `include/monattk.h`; all M1/M2/M3 definitions and species usages; all `prop.h` properties/masks. Reconcile actual counts rather than relying on stale47/50/16/17 coverage claims. Expand one tag/context contract and ≤20-member discovery packets; test hero->monster, monster->hero, monster->monster, passives and effect reuse independently.

For levels enumerate pinned `dat/*.lua` with `git ls-tree`, classify the historical 7 shared library/fill exclusions, reconcile 124 level definitions, and separately retain filler/library semantic obligations in DG04. Emit actual source-file IDs for every variant before DG05 ready: Minetown1–7, Mines End1–3, Sokoban1–4 x2, Big Room variants, Medusa1–4, all 13-role home/locate/goal/fillers, Oracle/Castle/Ludios, demon/fake towers/Vlad/Wizard, Valley/Sanctum/Planes/Astral/tutorial. Audit source lists instead of guessing names or freezing an unverified count. Validate DSL/data conversion against source facts without running Lua/C.

Enumerated catalogs have complete member references checked now; declared AT/AD/flags/properties/levels explicitly remain enumeration audits with no invented member counts. Whole-catalog closure requires extraction completeness, every member/context accepted or explicit reviewed scope exclusion, and public content placement/reachability. Newly discovered branches remain mandatory even if absent from543 rows.

## Carry every accepted broader gap forward

- P04a: Effective gear/temporary attributes; polymorph form formula; broader combat terms → EQ01, CB01, HE07.
- P04b: Effective STR; twohand/dual scaling/rings; broader polymorph/weapon contexts → EQ01, EQ05, HE07.
- P04c: Form/mount/legs/air modifiers; low-HP stamina/exercise; coin/loadstone/partial-stack prompts → EQ04, HE09, HE02, OB01.
- P05: Pw, regeneration/Sleepy/prayer properties, polymorph, effective CON and other contexts → HE04, EQ01, HE05, HE07.
- P06: Other public slots/forms; broader worn-target disrobing; special teleport/theft contexts → EQ02, AI03, DG03.
- P07: Other properties/slots, burning and secondary dragon effects; preexisting MC approximations → EQ03, P11d, CB02.
- P08: Rider/Ice-specific execution; corpse-with-contents burial; mextra/shop/quest/special identities; global next_ident wrap → OB04, DG07, DG06, AI05.
- P09: MSLOW/MFAST/gallop, polymorph/property acquisition, C linked-head ordering; death remaining-credit/same-pass-spawn exclusions source-inspected rather than dedicated tests → AI02, HE09, HE07, EQ03.
- P10: Other groups, mavenge unrepresented, inventory/special creation, habitats/quests/levels, other wand contexts, exact global ID wrap → AI01, AI03, DG07, DG05, OB07, AI05.

P02/P03 and HP/item-generation acceptance records remain reusable bounded evidence. Broader HP `adj_lev`/planes, direct name-based behaviors, corpse probability/payloads, fuel/food/glob variants and artifact gift/power contexts are covered by AI01/AI05, OB02–04/10 and their member children. Earlier supported containers/room selection do not close all object-generation contexts.

## Waves and first candidate

The existing M1–M8 roadmap release groups remain unchanged. Execution waves interleave prerequisites within those groups; the P11 fire candidate does not reorder release milestones. Wave0 is source contracts/catalog enumeration and bounded inventory reconciliation; no status refresh by inference. The first execution-ready candidate path is **P11a source audit → independent contract review → explicit selected-item/charge/cancel child**. Reuse P10 accepted ordinary create-monster evidence, and check current ActionAst/frontend selection first. P11b cannot proceed without traversal/terrain contract; P11c requires audited effective resistance/reflection; P11d requires owned destruction and terrain contracts. These dependencies are separate units, not hidden fire scope.

Wave1 establishes EQ01–03, OB01–02, HE01–03, AI01 and DG01, plus independent UI01 command audits. Wave2 expands traversal/resistance, timed/lifecycle systems, AI movement/inventory/pets, eating/progression/properties, vision/traps/topology. Wave3 fills per-member effects/AT-AD contexts and maps/NPCs/quests. Wave4 integrates Wizard/endgame, endings/text/options/bones and cross-stream regression closure. Dependency edges in the registry take precedence over these broad waves; UI reachability/save checks run within every earlier child, never wait until wave4. No concurrent writers.

## Execution and acceptance

Coordinator plus at most one active child; one writer/one Cargo runner, no nested fleet. Normal implementer Sol/medium; select Sol/high at contract time for new timers/scheduling/cross-level lifecycle/C ordering. Fresh Sol/high semantic review; optional Luna/high only for narrow missing facts/≤20 rows. Preserve existing loop soft envelopes:4–6k structural,8–12k normal,≤16k difficult before splitting; usage unknown if unreported. Two unsuccessful same-model repairs/three total, one narrow gpt-6-astra/low advice event after the second unsuccessful repair of the same defect, with an optional second turn at medium and two adviser turns maximum; counters never reset. Infrastructure gets one justified changed-hypothesis retry.

Independent fixtures must derive literal expected outcomes and prescribed draw bounds/order from pinned source, not production expected-value helpers. Include success/failure and boundaries, actual public native consumption, source-correct turn/RNG ordering, and mid-operation save/old-load/native continuation. Inspect owned canonical/custom definitions/renames/overrides, Arc ruleset reads, no HashMap/HashSet iteration effects, canonical sorted JSON/no definition floats, serde(default) and load tests, WASM dependency limits, and assertions preserved. Source inspection-only exclusions such as P09 death-credit/same-pass spawn receive dedicated regressions under AI02 where they matter. When definitions, actions or serialized public surfaces change, require applicable old-pack/old-save loads, schema and generated-binding freshness, and canonical schema/hash and wire-format compatibility checks. Record applicability in the child contract.

After focused checks and fresh review PASS, coordinator runs existing full 12 gates: fmt, Clippy, workspace excluding Python (proving all 3 golden tests), WASM build, Python check, no-static-table audit, extraction, generated Rust freshness, behavior coverage, parity-summary, docs links and Lean. Use local NAT/domain tools and uv/.venv as existing loop defines, preserve complete logs/exits externally, stop dependent Rust checks on compile failure, and do not duplicate goldens when workspace log proves them.

Accepted 999-test evidence can be reused for documentation-only planning if 342 production inputs/HEAD/source hashes are unchanged; rerun changed docs/summary checks. Implementation acceptance binds reviewed diff and full gates to sorted input hashes/digest, reports adjusted setup/assertions, exclusions/successors and repair counts. Coordinator updates obligations/progress before/after each task and commits only after green gates with required trailers on authorized `feat/monster-hp-d8`; no worker commits/golden edits.

Goldens remain frozen by default. The approved 16 values (4 RUNS/9 LOGS/3 COMBAT) apply only to completed P04–P10. HP/item exceptions and the broad old handover text do not authorize future automatic updates. A future intentional mismatch requires an exact task-specific reviewed proposal and recorded user decision; test logic/assertions never weaken to achieve green.

Full-stream closure requires every inventory row reconciled, every pinned family branch/member/context accounted, public frontend applicability tested, continuation verified and no scoped divergence silently omitted. The 543 rows are a traceability index, not a total C branch count. Optional replay/window-port/binary-file targets are separate decision work, and no C differential executable is introduced.

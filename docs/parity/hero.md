# Gap audit: THE HERO (NetHack 5.0 C vs NetHackED)

Scope: exper.c, attrib.c, u_init.c, eat.c, pray.c, spell.c, weapon.c, polyself.c, were.c, timeout.c, sit.c, steed.c, worn.c/do_wear.c, wield.c, hack.c, engrave.c, artifact.c, music.c, fountain.c.
Method: grep and read of crates/nethacked-{core,sim,data,types,arena} (plus tui where it bypasses the sim), cross-checked against docs/formal-mechanics-spec.md "Known divergences" (KD).
Paths: `core/` = crates/nethacked-core/src, `sim/` = crates/nethacked-sim/src, `data/` = crates/nethacked-data/src, `types/` = crates/nethacked-types/src/lib.rs.

Context: the player-facing action set is the `ActionAst` enum (core/ast.rs). It has no Wear/TakeOff/PutOn/Remove, #enhance, #sit, #invoke, #turn, #monster, #twoweapon, travel or run, throw (only Fire), play or #chat. The hero state is `types::Hero` (lines 956-967) plus `SimulationWorld` fields (sim/world.rs:66-125). It has no XP, attributes, Unchanging, sickness, strangulation, wounded legs, blindness or levitation timers.

| Mechanic | C reference | NetHackED status | Evidence | Player impact |
|---|---|---|---|---|
| **Experience (exper.c)** | | | | |
| XP awarded for kills | exper.c:experience (85), more_experienced (169) | ❌ MISSING | sim/combat.rs:386-517 `on_actor_killed` gives alignment and a corpse, no XP; grep `experience\|u.uexp` in sim: no match | Killing monsters never advances the hero |
| Level gain (newexplevel/pluslvl) | exper.c:newexplevel (300), pluslvl (307) | ❌ MISSING | hero `ActorRecord.level` is set to 1 (data/ruleset.rs:536) and never written for the player; grep `pluslvl\|level_up`: no match | Hero stays XL1 for the whole game; quest gate (XL14, core/quest.rs:7) is unreachable |
| HP growth per level (newhp) | attrib.c:newhp (1080) | ❌ MISSING | no level-up path; only +1 max HP per sacrifice (sim/actions/religion.rs:154) | Max HP never scales; late game is unwinnable by HP |
| Pw growth per level (newpw/enermod) | exper.c:newpw (45), enermod (26) | ❌ MISSING | sim/world.rs:190-195 fixed per-role Pw; no other write to `player_max_pw` | Casters never get more energy |
| Level loss / drain (losexp) | exper.c:losexp (207) | ❌ MISSING | grep `losexp\|drain`: no match; KD says monster AD_DRLI is physical damage only | No level-drain threat from vampires or wraiths |
| Potion/wraith gain level | exper.c:pluslvl via potion.c / eat.c | ❌ MISSING | no gain-level potion in data/items.rs; wraith corpse not special | Missing a classic progression tool |
| Score/rank titles by XL | exper.c / botl.c rank | ❌ MISSING | no XL progression | Status line rank never changes |
| **Attributes (attrib.c)** | | | | |
| St/Dx/Co/In/Wi/Ch tracked | attrib.c:acurr (1200), init_attr (723) | ❌ MISSING | KD "Attributes: no Charisma, Constitution..."; sim/world.rs:13 `DEFAULT_PLAYER_CON = 10`; no attribute fields in Hero | No stat-based character differences at all |
| Str/Dex to-hit and damage (abon/dbon) | weapon.c:abon (950), dbon (993) | ❌ MISSING | KD "`abon()` omitted"; core/combat.rs:55 `to_hit_value` has no attribute term | Strong fighters hit no harder; low-XL +1 to-hit is absent |
| Exercise / abuse (exerchk) | attrib.c:exercise (489), exerper (521), exerchk (598) | ❌ MISSING | grep `exercise\|exerchk`: no match | No stat training from actions or hunger |
| Attribute gain/loss (adjattrib, gainstr, losestr) | attrib.c:117/203/221 | ❌ MISSING | no attributes; no gain-ability/restore-ability or poison Str loss | Poison and potions never touch stats |
| Carry capacity from St+Con | hack.c:weight_cap (4295) | 🟡 PARTIAL | core/inventory.rs:139 `weight_cap` exists but is called only from tests and lib re-export; no sim call (grep `weight_cap` in sim: no match) | Unlimited carrying in practice |
| Luck value and to-hit bonus | attrib.c:change_luck (411); uhitm luck term | 🟡 PARTIAL | sim/world.rs:116 `player_luck`; used in to-hit sim/combat.rs:145; changed only by killing the quest leader or guardians (sim/combat.rs:430,446) | Luck matters for to-hit but almost nothing changes it (no mirrors, unicorn gems, prayer or sacrifice effects) |
| Luck timeout and luckstone | timeout.c:nh_timeout (595-620), attrib.c:set_moreluck (441) | ✅ PRESENT | sim/world.rs:566-586; core/mines.rs:132-149; called from sim/actions/mod.rs:136-141 | Matches C, except god-anger 300-turn period is hard-coded false (KD: base luck 0, no moon phase) |
| Moon phase / Friday 13 luck | allmain.c / attrib.c u_init luck | ❌ MISSING | KD "luck ignores every source other than the luckstone (base luck is 0)" | Minor; flavor messages and +1 luck lost |
| **Starting character (u_init.c)** | | | | |
| Starting HP (role + race infix) | u_init.c:u_init (997) via attrib.c:newhp; role.c hpadv | 🟡 PARTIAL | data/roles.rs:59-158 fixed `base_hp` per role, race not added; e.g. Valkyrie 18 (C 14+2=16), Barbarian 20 (C 16), Rogue 14 (C 12), Archeologist 14 (C 13) | Starting durability differs from C by role; race has no effect |
| Starting Pw | exper.c:newpw; u_init.c:1408-1411 | 🟡 PARTIAL | sim/world.rs:190-195 hard-coded (Wizard 25, Healer 20, Knight/Monk 10, others 5) | Approximate; not rolled from role/race enadv |
| Starting inventory | u_init.c:ini_inv (1301) | 🟡 PARTIAL | KD "Starting inventories still differ"; data/lib.rs:94 role items; every role also gets a food ration (sim/world.rs:198) | Wrong kit for several roles (e.g. Valkyrie has no small shield, so AC 10) |
| Starting spells / pre-known objects | u_init.c:knows_object (575), ini_inv spellbook | 🟡 PARTIAL | sim/world.rs:191-192 gives Wizard force bolt and Healer cure light wounds only | Only 2 roles start with spells; no pre-identified items |
| Starting attributes / alignment record | attrib.c:init_attr; u_init alignment record | 🟡 PARTIAL | alignment record per role is present (KD); attributes MISSING | Alignment record is right; stats are absent |
| Starting intrinsics (role/race, e.g. Valk cold res, Monk fast) | attrib.c role/race intrinsic tables (adjabil) | 🟡 PARTIAL | data/roles.rs:272-292 race intrinsics only (elf see-invis, orc poison res); no role intrinsics or level-gated ones | Valkyrie lacks cold res and stealth; Monk lacks Fast, etc. |
| Starting pet | u_init.c / dog.c makedog | ❔ UNSURE | not in hero files audited; pet code exists (sim/monsters.rs:660ff) | Out of scope; see the pets audit |
| **Eating (eat.c)** | | | | |
| Hunger states and thresholds | eat.c:newuhs (3367) | ✅ PRESENT | core/nutrition.rs:24-39 matches eat.c:3362/3437 (Con fixed at 10) | Status thresholds are correct |
| Hunger rate (rings, regen, amulet, encumbrance) | eat.c:gethungry (3168) | 🟡 PARTIAL | core/nutrition.rs:55 flat -1/turn; sim/turns.rs:74 | No extra hunger from regeneration, rings, amulets or casting |
| Fainting | eat.c:newuhs fainting | 🟡 PARTIAL | sim/turns.rs:84-107 drains 1 HP/10 turns (KD "Fainting") | Hero never actually faints and loses turns; HP loss is invented |
| Starvation death | eat.c:newuhs (3437) | ✅ PRESENT | sim/turns.rs:108-119 | Matches threshold |
| Multi-turn eating / interruption | eat.c:start_eating (2027), bite (3138), eatfood (524) | ❌ MISSING | sim/actions/items.rs:750-838 eats instantly in one action | No meal-interruption risk; eating in combat is free |
| Per-food nutrition | objects.h oc_nutrition, eat.c | 🟡 PARTIAL | sim/actions/items.rs:761-772 catalog value, else ration 800, apple 50, corpse flat 400 | Corpse nutrition is not species-based |
| Species-specific corpses | eat.c:eatcorpse (1860); mon.c corpse creation | ❌ MISSING | sim/combat.rs:507-512 drops a generic `Corpse` record with `corpse_race: None` and fixed name | Every kill leaves an identical "corpse" |
| Corpse intrinsics (givit, intrinsic_possible) | eat.c:givit (1008), intrinsic_possible (895), cpostfx (1134) | 🟡 PARTIAL | core/nutrition.rs:67-79 name table, always granted (no level-based chance); sim/actions/items.rs:797-834; generic corpses never match except via the "ant/kobold/orc" substring hack (823-833) | Intrinsics are effectively unobtainable in play; when obtained, they are 100% instead of a level-based chance |
| Special corpse effects (lizard, acidic, newt Pw, wraith, nurse, mimic, stalker invis, cockatrice stoning, were) | eat.c:cprefx (796), cpostfx (1134) | 🟡 PARTIAL | only lizard cures stoning (sim/actions/items.rs:818-822) | Most corpse tactics are absent; eating a cockatrice is safe |
| Cannibalism penalty | eat.c:maybe_cannibal (763) | 🟡 PARTIAL | sim/actions/items.rs:782-787 message only, compares with hard-coded "human", corpse_race always None, so it never fires | No luck/alignment penalty or aggravate monster |
| Rotten/tainted corpses (food poisoning) | eat.c:eatcorpse rotted / rottenfood (1818) | 🟡 PARTIAL | sim/actions/items.rs:788-795 prints "tainted" only; no sickness, blinding, stun or unconsciousness | Old corpses are safe to eat |
| Choking when satiated | eat.c:choke (245) | ❌ MISSING | grep `choke`: no match; nutrition is capped at 2000 (sim/actions/items.rs:773) | No overeating death; no Satiated risk |
| Tins (opening, contents, tinning kit) | eat.c:start_tin (1728), consume_tin (1533) | ❌ MISSING | grep `tin\b`: no match; no tin item in data/items.rs | Tins absent |
| Eating conducts | eat.c:eating_conducts (581) | ✅ PRESENT | sim/actions/items.rs:781 `record_eat_meat` | Vegan/vegetarian tracking works for meat |
| Vegan food, eggs, royal jelly, lembas etc. | eat.c:fpostfx (2515) | ❌ MISSING | data/items.rs has only FoodRation, Apple, Corpse | Very small food variety |
| **Prayer and sacrifice (pray.c)** | | | | |
| Prayer timeout value and decay | pray.c:can_pray (2124); prayer_timeout ~300 random; decrements in allmain | 🟡 PARTIAL | sim/actions/religion.rs:37 fixed 300; core/religion.rs:20 -1/turn (mod.rs:131-134) | Timeout is deterministic, not rnz(350); no luck effect on decay |
| Trouble detection and fixing (low HP, hunger, stoning, sliming, lycanthropy, cursed items...) | pray.c:in_trouble (198), fix_worst_trouble (373) | ❌ MISSING | sim/actions/religion.rs:11-104: no trouble evaluation; full heal only on a co-aligned altar (41-46) | Prayer does not save you from low HP, starvation or stoning off-altar |
| Prayer success depends on luck/alignment/anger | pray.c:can_pray p_type | ❌ MISSING | success depends only on `prayer_timeout > 0` (religion.rs:22) | Prayer is predictable; luck/alignment ignored |
| Praying too soon: smiting / god anger | pray.c:angrygods (704), god_zaps_you (610) | 🟡 PARTIAL | religion.rs:22-33 fixed 8 damage (floored at 1 HP), -3 favor; no ugangr, no lightning/disintegration, no curse items | Praying too soon is mild and never lethal |
| God anger state (ugangr) and its effects | pray.c:gods_upset (1436) | ❌ MISSING | `types::DivineState` has favor/prayer_timeout/gift_count only (types 287); `luck_decay_period(..., false)` sim/world.rs:567 | No lasting divine anger |
| Water prayer (bless/curse water) | pray.c:water_prayer (1387) | 🟡 PARTIAL | religion.rs:49-63 blesses one uncursed water if favor > 5; never curses | Cross-aligned unholy water is absent |
| Sacrifice value by corpse difficulty and age | pray.c:dosacrifice (1854), offer_corpse (1959) | 🟡 PARTIAL | religion.rs:131-135 flat 250/100 "nutrition"; core/religion.rs:26-59 +3/+1 favor; non-corpses are also accepted | Any item can be sacrificed; corpse age/difficulty ignored |
| Cross-altar conversion | pray.c:offer_corpse altar conversion | 🟡 PARTIAL | core/religion.rs:32-39 always converts (+2 favor) | No luck roll, no failure, no -1 alignment for converting; no temple priest anger |
| Same-race sacrifice | pray.c:sacrifice_your_race (1698) | ❌ MISSING | no race on corpses | No demon summons or alignment shift |
| Sacrifice gifts (bestow_artifact) | pray.c:bestow_artifact (1781) | 🟡 PARTIAL | core/religion.rs:44-51: Excalibur once at favor >= 15, for every role/alignment; religion.rs:161-185 untyped item record | Wrong gift for most roles; no gift-chance formula, no skill unlock |
| Crowning (gcrownu) | pray.c:gcrownu (805) | ❌ MISSING | `Messages::divine_crowning` is only the gift message (religion.rs:183); grep `gcrownu`: no match | No Hand of Elbereth, no crowning intrinsics or artifact |
| Sacrifice reducing prayer timeout / luck gain | pray.c:offer_corpse (luck, timeout) | ❌ MISSING | resolve_sacrifice never touches prayer_timeout or luck | Sacrifice has no luck payoff |
| Unicorn sacrifice | pray.c:offer_corpse unicorn | ❌ MISSING | no unicorn corpse species | n/a |
| High altar / Amulet offering | pray.c:dosacrifice Astral | ✅ PRESENT | religion.rs:186-230 (HighAltar branch) | Ascension offering is modelled (endgame audit) |
| **Spells (spell.c)** | | | | |
| Learning from spellbooks (study time, difficulty, failure) | spell.c:study_book (468), learn (356), cursed_book (130) | 🟡 PARTIAL | sim/actions/items.rs:693-732 instant learn; unknown book becomes cure light wounds; no level/Int check or cursed-book effects | No risk in reading books |
| Spell catalog | spell.c spelleffects (1385) | 🟡 PARTIAL | core/magic.rs:9-14 four spells; only two books exist (data/items.rs:62-63) | About 4 of 40+ spells |
| Spell failure rate (armor, Int/Wis, role emergency spell) | spell.c:percent_success (2173) | ❌ MISSING | sim/actions/items.rs:859-870 always succeeds when Pw is enough | Casting in plate mail is free of penalty |
| Pw cost = 5 x spell level | spell.c:spelleffects energy | 🟡 PARTIAL | core/magic.rs:17-24 ad-hoc (force bolt 5, MM 10, CLW 5, EH 15) | Costs do not match C |
| Spell effects (force bolt 2d6, etc.) | spell.c:spelleffects; zap.c | 🟡 PARTIAL | items.rs:872-918 fixed 18/14 damage, range 6, stops at the first target | Deterministic big hits; no skill or level scaling |
| Spell retention / forgetting | spell.c:age_spells (669), losespells (1763) | ❌ MISSING | `known_spells` retention 20000 is never decremented; core/magic.rs:42 `decay_retention` is unused in sim | Spells are never forgotten |
| Energy regeneration | allmain.c:u_regen_pw / spell.c energy | ❌ MISSING | no write to `player_pw` except casting (items.rs:870) | Pw never refills, so casters get a few casts per game |
| Spell skills (attack, healing...) | weapon.c spell skills | ❌ MISSING | `SkillClass` has no spell schools (types 908-916) | n/a |
| **Weapon skills (weapon.c)** | | | | |
| Skill to-hit/damage bonus | weapon.c:weapon_hit_bonus (1545), weapon_dam_bonus (1644) | 🟡 PARTIAL | core/skills.rs:3-33; KD notes the bare-handed damage table differs; martial arts absent | Roughly right for weapons; Monk/Samurai martial arts weaker |
| Skill practice (use_skill) | weapon.c:use_skill (1424) | ❌ MISSING | grep `use_skill\|practice`: no match | Skills never improve through use |
| Skill slots and #enhance | weapon.c:enhance_weapon_skill (1329), can_advance (1156) | 🟡 PARTIAL | core/skills.rs:35-57; TUI-only modal tui/src/main.rs:490-565 mutates world outside actions; `available_slots` is never incremented (only default 0) | #enhance never works in practice |
| Skill set breadth / role max | weapon.c:skill_init (1738); role skill tables | 🟡 PARTIAL | `SkillClass` 7 entries (types 908-916); no role caps | Most weapon classes have no skill |
| Weapon hitval/dmgval extras | weapon.c:hitval (149), dmgval (216) | 🟡 PARTIAL | KD "Hero weapon damage (simplified C dmgval)"; skill selected by name substrings (KD) | Blessed-vs-undead, silver, large-monster dice are missing |
| Two-weapon combat | wield.c:dotwoweapon (850), can_twoweapon (766) | ❌ MISSING | grep `twoweap`: no match | No dual-wielding |
| Skill drain/lose | weapon.c:drain_weapon_skill (1476) | ❌ MISSING | no match | n/a |
| **Wielding (wield.c)** | | | | |
| Wield command | wield.c:dowield (360), ready_weapon (169) | 🟡 PARTIAL | sim/actions/inventory.rs:105-132 sets any item as wielded, no checks | Can wield armor or food; no bimanual/shield conflict |
| Cursed weapon welding | wield.c:welded (1056) | ❌ MISSING | no BUC check in handle_wield | Cursed weapons are freely swapped |
| Swap weapon (x) / quiver | wield.c:doswapweapon (466), dowieldquiver (510) | 🟡 PARTIAL | quiver present (sim/actions/ranged.rs:12); no swap | No x-swap |
| Wielding cockatrice corpse barehanded | wield.c ready_weapon / touch_petrifies | ❌ MISSING | no species corpses | n/a |
| **Armor and accessories (do_wear.c, worn.c)** | | | | |
| AC from worn armor (find_ac) | do_wear.c:find_ac (2473) | ✅ PRESENT | core/ac.rs:202; sim/world.rs:477; KD "carried armor counts as worn" | AC is right for armor carried |
| Wear/Take off with delays, slot rules | do_wear.c:dowear (2432), dotakeoff (1833), canwearobj (2030) | ❌ MISSING | no Wear/TakeOff actions in core/ast.rs; carried = worn (KD) | You cannot choose what to wear; picking armor up "wears" it |
| Cursed armor can't be removed | do_wear.c:armoroff/select_off | ❌ MISSING | no removal at all | n/a |
| Rings (Ring_on/Ring_gone, effects) | do_wear.c:Ring_on (1242), Ring_gone (1455) | ❌ MISSING | no ring items in data/items.rs; no PutOn | No rings in the game |
| Amulets (Amulet_on: ESP, life saving, strangulation, reflection) | do_wear.c:Amulet_on (963) | ❌ MISSING | AmuletOfReflection item exists (data/items.rs:66), but nothing sets `intrinsics.reflection` from items; grep in sim: only reads (monsters.rs:464,606) | Sokoban prize amulet does nothing |
| Armor extrinsics (cloak of MR, SDSM reflection, speed/levitation boots, helm of telepathy...) | do_wear.c *_on functions; worn.c setworn (73) | ❌ MISSING | grep `magic_resistance` writes in sim: no match; levitation boots only an AC entry core/ac.rs:113 | Wizard's cloak of MR gives no MR; SDSM gives no reflection |
| Polymorph breaking armor | polyself.c:break_armor (1157) | ❌ MISSING | core/polymorph.rs:177-195 stub using "form % 2" | n/a |
| Monster worn armor AC | worn.c:find_mac (717) | ✅ PRESENT | sim/combat.rs:522 `defender_ac` | n/a (monster side) |
| **Polymorph and lycanthropy (polyself.c, were.c)** | | | | |
| Polyself into a real monster form | polyself.c:polyself (469), polymon (735) | 🟡 PARTIAL | sim/actions/items.rs:319-340 sets dummy form `monster_id: 1`, 20 HP; no species stats, attacks or abilities | Polymorph is just an HP buffer |
| HP buffer / rehumanize on form death | polyself.c:rehumanize (1367); hack.c:losehp (4256) | ✅ PRESENT | core/polymorph.rs:141-175 matches C; Unchanging passed as false (KD) | Works as in C |
| Polymorph duration timeout | polyself.c / timeout.c u.mtimedone | ❌ MISSING | `PolymorphForm.duration` 100 is never decremented (grep `duration` in sim: only items.rs:333) | Polymorph lasts until form death |
| System shock / newman | polyself.c:newman (336) | ❌ MISSING | no match | No risk of level/HP reroll or death |
| Polyform abilities (#monster: breathe, spit, gaze, hide, summon, web, mind blast) | polyself.c:dobreathe (1421) ... domindblast (1894) | ❌ MISSING | no #monster action | n/a |
| Lycanthropy infection from were bites | were.c:you_were (192), set_ulycn (232); mhitu AD_WERE | ❌ MISSING | `LycanthropyState` exists (types 944) but is never set (always None, sim/world.rs:362) | No lycanthropy threat |
| Lycanthropy cure (wolfsbane, holy water, prayer) | were.c:you_unwere (213) | 🟡 PARTIAL | core/polymorph.rs:197 `cure_lycanthropy` exists and is unused; SprigOfWolfsbane item exists, eating it has no effect | Unreachable |
| Were monsters changing form / summoning | were.c:were_change (9), were_summon (142) | ❌ MISSING | no match | n/a |
| **Timeouts (timeout.c)** | | | | |
| Stoning countdown and death | timeout.c:stoned_dialogue (137); nh_timeout | 🟡 PARTIAL | core/afflictions.rs:10-19 countdown and death, sim/turns.rs:32-41; nothing ever sets `petrification` (KD: gaze/AD_STON not modelled) | Stoning never starts; cures are cosmetic |
| Stoning cures (lizard, acidic corpse, potion of acid, prayer) | eat.c / potion.c / pray.c | 🟡 PARTIAL | lizard (items.rs:818), acid potion (items.rs:342); not prayer | Moot while stoning cannot start |
| Sliming countdown, death and fire cure | timeout.c:slime_dialogue (389), burn_away_slime (448) | 🟡 PARTIAL | core/afflictions.rs:21-27; fire breath cures (sim/monsters.rs:643); nothing sets `sliming` | Sliming never starts |
| Strangulation | timeout.c:choke_dialogue (295); amulet of strangulation | ❌ MISSING | grep `strangl`: no match | n/a |
| Sickness / food poisoning / illness death | timeout.c Sick handling; eat.c rotten | ❌ MISSING | no sick field in Hero | Rotten food and Demogorgon sickness are harmless |
| Vomiting | timeout.c:vomiting_dialogue (197) | ❌ MISSING | no match | n/a |
| Levitation timeout and landing | timeout.c:levitation_dialogue (353) | ❌ MISSING | `intrinsics.levitation` is never set (grep `levitation = true` in sim: no match); only read for traps (movement.rs:328) | No levitation gameplay |
| Confusion/stun/hallucination timers | timeout.c:nh_timeout HConfusion etc. | 🟡 PARTIAL | sim/ad_effects.rs `tick_hero_afflictions`: stun, confusion, blindness and sleep run down per turn with the C end messages; set by monster hits; hallucination never set | Stun/confusion randomize movement; sleep skips turns |
| Blindness timer | timeout.c Blinded | 🟡 PARTIAL | `intrinsics.blind` bool, no duration; sources unclear | Permanent if set; no timed blindness |
| Sleep / fall_asleep | timeout.c:fall_asleep (951), sleep_dialogue (268) | ❌ MISSING | no match | Sleep attacks or traps never knock the hero out |
| Speed intrinsic effect | timeout.c / allmain.c moveamt (Fast) | ❌ MISSING | potion of speed sets `intrinsics.fast` (items.rs:315) but the scheduler never reads it (grep `.fast` in core/energy.rs: no match) | Speed potion is a placebo |
| Wounded legs | timeout.c Wounded_legs | ❌ MISSING | `weight_cap` has a param (core/inventory.rs:139) but no state | n/a |
| HP regeneration | allmain.c:u_regen_hp (with timeout/encumbrance rules) | ❌ MISSING | no hero HP increase per turn in sim/turns.rs; only potions, spells, prayer | The hero never heals naturally; resting is useless |
| **Sitting and thrones (sit.c)** | | | | |
| #sit (on traps, items, water, lava) | sit.c:dosit (400) | ❌ MISSING | no Sit action in core/ast.rs | n/a |
| Throne effects (wish, genocide, gold, curse...) | sit.c:throne_sit_effect (39), special_throne_effect (238) | ❌ MISSING | no Throne tile (types 364-400) | No throne rooms |
| Laying eggs while polymorphed | sit.c:lay_an_egg (358) | ❌ MISSING | no match | n/a |
| rndcurse / attrcurse | sit.c:rndcurse (569), attrcurse (644) | ❌ MISSING | no match | n/a |
| **Riding (steed.c)** | | | | |
| Mount steed (skill check, saddle needed, failure damage) | steed.c:doride (178), mount_steed (197) | 🟡 PARTIAL | sim/actions/ranged.rs:132-157: any tame monster, saddle assumed (`saddle_equipped: true`, "simplified saddle check") | Ride any pet without a saddle and with no fail chance |
| Saddling | steed.c:can_saddle (26), use_saddle (36) | ❌ MISSING | no saddle item | n/a |
| Steed movement speed | steed.c / allmain.c | 🟡 PARTIAL | sim/actions/movement.rs:140-155 cost from steed speed | Roughly right |
| Riding skill, kick steed, steed exercise, falls | steed.c:kick_steed (402), exercise_steed (387) | ❌ MISSING | no match | n/a |
| Dismount and landing spot | steed.c:dismount_steed (576), landing_spot (460) | 🟡 PARTIAL | sim/actions/ranged.rs:159-178 | No forced dismount or damage |
| **Movement (hack.c)** | | | | |
| Basic movement and walls | hack.c:domove_core (2712), test_move (991) | ✅ PRESENT | sim/actions/movement.rs:134ff | Works |
| Diagonal movement through doorways forbidden | hack.c:test_move (doorway diagonal) | ❌ MISSING | grep `diagonal` in movement.rs: no match | Hero can move diagonally into or out of doors |
| Boulder pushing | hack.c:moverock (336) | ✅ PRESENT | movement.rs:195-246 + core/sokoban.rs | Push, pit fill and block work; no squeeze-past or force-fight-the-boulder |
| Travel command `_` | hack.c:findtravelpath (1266) | ❌ MISSING | no Travel action; core/pathfinding.rs is only for monsters | No travel |
| Running (G/shift-move) and lookaround | hack.c:lookaround (3898) | ❌ MISSING | no match | Tedious one-step movement |
| Swimming/drowning, water/lava entry | hack.c:pooleffects (3233), swim_move_danger (1885); trap.c drown | ❌ MISSING | Pool/Moat are impassable unless frozen (types 417) | Water is just a wall; no drowning danger |
| Encumbrance effects (speed, can't climb stairs, overexertion) | hack.c:calc_capacity (4372), near_capacity (4385), overexertion (3051) | ❌ MISSING | `encumbrance_tier` unused in sim | Weight is irrelevant |
| Peaceful swap / displacing pets | hack.c:domove_swap_with_pet (2141-2176) | ✅ PRESENT | movement.rs:46-121 (KD "Peaceful swap") | Close to C |
| Traps on entry | hack.c:spoteffects (3312) | ✅ PRESENT | movement.rs:322ff | See the traps audit |
| Engraving wipe on movement | hack.c:u_wipe_engr / engrave.c:wipe_engr_at (272) | 🟡 PARTIAL | movement.rs:288-310 smudges a dust engraving on step-off | C wipes on fighting, not on walking; these differ |
| **Engraving and Elbereth (engrave.c)** | | | | |
| Engrave command with media (dust, burn, carve) | engrave.c:doengrave (962), make_engr_at (409) | 🟡 PARTIAL | sim/actions/engrave.rs:9-38: medium chosen freely by the caller; no wand/athame/tool requirement, always 1 action | Burned Elbereth for free with no wand |
| Elbereth scaring rules | monmove.c:onscary (240-302) | 🟡 PARTIAL | core/engraving.rs:68-110; sim/monsters.rs:324; Gehennom/endgame suppression and minion immunity not modelled (doc comment) | Elbereth works in Gehennom, where it should not |
| Elbereth erased when the hero attacks on it | engrave.c / uhitm.c (5.0 erase on attack) | ❌ MISSING | sim/combat.rs: no engraving wipe (only a comment at 347) | Fight forever from an Elbereth square |
| Reading engravings | engrave.c:read_engr_at (319) | ✅ PRESENT | movement.rs:313-320 | Works |
| Random engravings / headstones | engrave.c:random_engraving (52) | ❔ UNSURE | headstone in agent/bones only | Minor |
| Engraving degradation on engrave with fingers | engrave.c wipeout_text | 🟡 PARTIAL | core/engraving.rs:31-64 durability counter, text never garbles | Elbereth never becomes "Elbcreth" |
| **Artifacts (artifact.c)** | | | | |
| Artifact damage bonus (spec_dbon) | artifact.c:spec_dbon (1091), spec_applies (1009) | 🟡 PARTIAL | core/artifacts_wands.rs:11-38 flat bonuses (e.g. Mjollnir +12, Tsurugi +16) instead of d(n) vs a target type | Artifacts are generically strong; not target-specific |
| Artifact to-hit (spec_abon) | artifact.c:spec_abon (1076) | ❌ MISSING | no match | n/a |
| Vorpal Blade / Tsurugi beheading | artifact.c:artifact_hit (1447) | 🟡 PARTIAL | sim/combat.rs:200-205, 313 1-in-20 behead | Close for Vorpal; Tsurugi bisection absent |
| Magicbane special effects | artifact.c:Mb_hit (1249) | ❌ MISSING | flat +4 only | No cancel/scare/stun |
| Artifact carried intrinsics | artifact.c:set_artifact_intrinsic (716) | ❌ MISSING | no match | Quest artifacts grant nothing (no MR or reflection) |
| #invoke | artifact.c:doinvoke (1749), arti_invoke (2131) | ❌ MISSING | no Invoke action | Quest artifacts are inert |
| Artifact blast / touch_artifact | artifact.c:touch_artifact (908) | ❌ MISSING | no match | Cross-aligned artifacts are safe to use |
| Excalibur by dipping a long sword in a fountain | fountain.c:dipfountain (394) | ❌ MISSING | no fountains | Classic Knight/lawful route missing |
| Artifact light / speaking | artifact.c:artifact_light (2264), arti_speak (2279) | ❌ MISSING | no match | Minor |
| **Music (music.c)** | | | | |
| Playing instruments (#apply flute/horn/harp/drum/bugle) | music.c:do_play_instrument (759), do_improvisation (503) | ❌ MISSING | no instrument items in data/items.rs; sim/actions/inventory.rs:312ff apply handles bell/candelabrum/candles only | No instruments |
| Passtune / drawbridge opening by tune | music.c do_play_instrument (passtune) | ❌ MISSING | grep `passtune`: no match | Castle drawbridge is opened another way (endgame audit) |
| Earthquake, sleep, charm, taming effects | music.c:do_earthquake (344), put_monsters_to_sleep (85), charm_monsters (196) | ❌ MISSING | no match | n/a |
| **Fountains and sinks (fountain.c)** | | | | |
| Quaffing from a fountain | fountain.c:drinkfountain (243) | ❌ MISSING | no Fountain tile (types 364-400) | No fountains anywhere |
| Dipping into a fountain (Excalibur, water moccasins, nymph, demon wish) | fountain.c:dipfountain (394), dowaterdemon (64) | ❌ MISSING | sim/actions/items.rs:94 `handle_dip` dips "into_water" anywhere without terrain | Dip works with no water source |
| Fountain drying up / gems | fountain.c:dryup (201), dofindgem (165) | ❌ MISSING | no match | n/a |
| Sinks (quaff, kick, ring identification, dip) | fountain.c:drinksink (595), dipsink (716), breaksink (581) | ❌ MISSING | no Sink tile | No sink ring-ID trick |

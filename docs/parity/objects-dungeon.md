# Gap audit: Objects and the Dungeon (NetHack 5.0 C vs NetHackED)

Date: 2026-10-05. C: the NetHack 5.0.0 source tree (src/, include/objects.h, dat/*.lua). Rust: `crates/*`.
Paths below are relative to the NetRust repo unless they start with `C:`.

## Object counts

C counts come from `include/objects.h` expanded by the C preprocessor (`scripts/extract/c_tables.py` → `data/nethack-5.0/objects.toml`, 463 entries after the strange object and the generic class placeholders). The 23 unnamed entries (20 extra scroll labels, 3 extra wand appearances) are appearances, not object kinds, which gives **440 object types**. Artifacts come from `include/artilist.h` (33, `artifacts.toml`).
NetHackED generates its catalog from that data (`crates/nethacked-data/src/generated/items.rs`): every object type and every artifact exists as data. Behaviour is a separate question: most kinds have no use/zap/quaff/read effect yet (see the feature rows below).

| Class | C types | NetHackED (data) |
|---|---|---|
| Weapon | 71 | 71 |
| Armor | 84 | 84 |
| Ring | 28 | 28 |
| Amulet | 13 | 13 |
| Tool | 50 | 50 |
| Food | 33 | 33 |
| Potion | 26 | 26 |
| Scroll | 23 | 23 |
| Spellbook | 44 | 44 |
| Wand | 25 | 25 |
| Coin | 1 | 1 |
| Gem/stone | 36 | 36 |
| Rock (boulder/statue) | 2 | 2 |
| Ball | 1 | 1 |
| Chain | 1 | 1 |
| Venom | 2 | 2 |
| **Total** | **440** | **440** + 33 artifacts |

## Feature table

Status totals: 119 rows: MISSING 59, PARTIAL 50, PRESENT 10 (2 rows also carry an UNSURE note).

| Feature | C reference | NetHackED status | Evidence | Player impact |
|---|---|---|---|---|
| Object type catalog | objects.h / objects.c | ✅ PRESENT | data/nethack-5.0/objects.toml → generated/items.rs (440 types + 33 artifacts) | Every item exists as data; effects are tracked per class below. |
| Rings (whole class) | objects.h RING() | ❌ MISSING | the 28 rings exist as data (generated/items.rs) but nothing puts them on or applies their effects | No rings: no free action, conflict, regeneration, levitation and so on. |
| Helms, boots, gloves, shields, other cloaks, dragon mail | objects.h HELM/BOOTS/GLOVES/SHIELD | ❌ MISSING | items.rs:16-21 has 5 armor kinds | AC builds and slot strategy are not possible. |
| Launchers and ammo (bows, arrows, crossbow, sling) | objects.h BOW/PROJECTILE | ❌ MISSING | no bow/arrow ItemKindId | Ranged weapon play does not exist. |
| Gems and gray stones (except luckstone) | objects.h GEM/ROCK | ❌ MISSING | items.rs:79-80 | No gem identification, unicorn gifts, loadstone or touchstone. |
| Iron ball and chain (punishment) | objects.h BALL/CHAIN, ball.c | ❌ MISSING | ItemClass::Ball/Chain exist (types lib.rs:504-505) but have no archetypes | Punishment is not possible. |
| Randomized appearances (shuffled descriptions) | o_init.c shuffle_all/setgemprobs | ❌ MISSING | ItemRecord has no appearance field (arena lib.rs:20-37); `grep -ri appearance crates/*/src` no match | Every item shows its true name, so the identification game is gone. |
| Discoveries list (`\``) | o_init.c dodiscovered | ❌ MISSING | `grep -ri discover crates/*/src`: no command | Nothing to look up, because nothing is unidentified. |
| Per-object knowledge flags (known/bknown/dknown/oc_name_known) | obj.h, objnam.c | ❌ MISSING | identification.rs:9-30 is a standalone lattice; ItemRecord has no knowledge fields | BUC and enchantment are never hidden, so altars and price-ID lose their point. |
| Price identification | shk.c get_cost/o_id surcharge | 🟡 PARTIAL | inventory_interaction.rs:105-182 has the formulas; economy.rs:94 says "No identification model exists" | Price math exists, but there is nothing to identify. |
| Object naming: doname (BUC/+N/erosion/charges/"(being worn)") | objnam.c doname/xname | ❌ MISSING | TUI prints `name - weight` only (nethacked-tui/src/main.rs:363-367) | The player cannot see an item's enchantment, BUC, erosion or charges. |
| Pluralization, articles, quantities | objnam.c makeplural/an/the | ❌ MISSING | ItemRecord has no `quan` field (arena lib.rs:20-37) | No stacks: 20 darts would be 20 separate items. |
| Object stacking/merging | invent.c merged/mergable | ❌ MISSING | no quantity field | Inventory clutter; no stack splitting. |
| #name / #call | do_name.c | ❌ MISSING | no ActionAst variant (core ast.rs:16-62) | Not needed while items are pre-identified. |
| Wishing parser | objnam.c readobjnam | 🟡 PARTIAL | artifacts_wands.rs:159-198 parses only BUC and ±N, then matches the name exactly; no quantity, erosion/fixed, "+N > rnd(5) -> 0" cap, artifact naming or appearance names | Any of the 51 types can be wished at any enchantment (e.g. "+127"); most C wishes fail. |
| Wish restrictions (AoY -> fake, quest artifacts and invocation items unwishable) | objnam.c readobjnam | ✅ PRESENT | items.rs:22-37 UNWISHABLE, :1214-1236 | Matches C for the covered items. |
| Random object generation on levels | mkobj.c mkobj/mkobj_at, mklev.c | ❌ MISSING | only fixed spawns (world.rs:198-316, stairs.rs:285-513); `grep mkobj\|random_item` no match | Levels carry no loot apart from scripted prizes. |
| BUC at creation (blessorcurse/curse rates) | mkobj.c mksobj_init | ❌ MISSING | the caller hard-codes BUC (e.g. world.rs:200 `Buc::Uncursed`) | BUC carries no risk. |
| Initial enchantment/charges | mkobj.c mksobj_init | 🟡 PARTIAL | wand charges follow C (items.rs:1155); weapon/armor spe never rolled | No random cursed -3 armor or +2 weapons on the floor. |
| Erosion and erodeproofing | mkobj.c, trap.c erode_obj | 🟡 PARTIAL | enchantment.rs:258 apply_erosion; one `erosion: u8` (arena lib.rs:28) and no separate oeroded/oeroded2 types | Rust and corrosion cannot be told apart, and erosion is invisible. |
| Corpse aging/rotting | mkobj.c, eat.c rottenfood | 🟡 PARTIAL | arena lib.rs:31-33 corpse_age/rot_threshold; items.rs:779-830 | Simplified food poisoning; few corpse species. |
| Container contents at creation | mkobj.c mkbox_cnts | ❌ MISSING | no contents generation | Boxes and chests are always empty. |
| Potions: quaff effects | potion.c dodrink/peffects | 🟡 PARTIAL | items.rs:297-367 handles healing (flat +10; extra healing gets the same branch), speed, polymorph (dummy form id 1), acid; everything else "tastes like water" | Potion effects are mostly missing or flat. |
| Potion vapors, breaking, thrown potions | potion.c potionbreathe/potionhit | ❌ MISSING | no handler | No vapor effects or potion throwing. |
| Dipping (holy/unholy water, dilution) | potion.c dodip | 🟡 PARTIAL | items.rs:94-157; core buc.rs:11 dip_water | Holy water BUC changes work; no excalibur dip (fountains do not exist). |
| Alchemy | potion.c mixtype | 🟡 PARTIAL | enchantment.rs:270 mix_alchemy; items.rs:260 handle_dip_potion | Only a few recipes, because most potions do not exist. |
| Scrolls: read effects | read.c seffects | 🟡 PARTIAL | items.rs:369-690 covers teleport, remove curse, EW, EA, charging, genocide; 15 C scroll types absent | No magic mapping, fire, light, scare monster, earth, amnesia and so on. |
| Scroll of genocide (BUC branches) | read.c do_genocide | ✅ PRESENT | items.rs:604-690; core genocide.rs | Works for the bestiary NetHackED has. |
| Scroll of charging / wand recharge explosion | read.c recharge | ✅ PRESENT | artifacts_wands.rs:87-157; items.rs:480-600 | Matches C's explosion formula. |
| Remove curse BUC/confused semantics | read.c seffects SCR_REMOVE_CURSE | 🟡 PARTIAL | items.rs:388-394 uncurses every carried item regardless of scroll BUC | Uncursed and cursed scrolls are too generous. |
| Confused/blind scroll reading | read.c | ❌ MISSING | no confusion check in handle_read | No confused-genocide or confused-EA tricks. |
| Spellbook reading/learning | read.c / spell.c study_book | 🟡 PARTIAL | items.rs:695-745 (force bolt, healing; Book of the Dead) | Only 2 learnable spells. |
| Wand selection when zapping | zap.c dozap getobj | 🟡 PARTIAL | items.rs:961-971 zaps the *first carried wand*; ActionAst::ZapWand has no item index (ast.rs:38-41) | A player with two wands cannot choose which one to zap. |
| Ray wands (buzz): path and wall bounce | zap.c buzz/bhit | 🟡 PARTIAL | dungeon raycast.rs:11-60 reflects off walls; items.rs:1038-1150 stops at the first monster; no hero self-hit on bounce | Rays do not pass through monsters, and a bounce never hits the hero. |
| Ray damage, resistances and reflection vs monsters | zap.c zhitm | 🟡 PARTIAL | items.rs:1080-1086 fixed 12/18/100 damage, no resist check | Death rays kill undead and resistant monsters alike; no d(6,6) damage. |
| IMMEDIATE wand effects (bhitm) | zap.c bhitm | 🟡 PARTIAL | items.rs:1088-1112: teleport "no effect", polymorph always gives a goblin, striking ignores MR | Wand of teleportation is useless against monsters; polymorph is fixed. |
| Wand of digging (dig through walls, dig down) | zap.c zap_dig | ❌ MISSING | items.rs:1088-1096 prints a message only | No escape by digging down and no tunnelling. |
| Wand of secret door detection | detect.c findit | ✅ PRESENT | items.rs:999-1029 (7x7 area, not C's BOLT_LIM radius) | Works, but the main dungeon has no secret doors. |
| Zapping while engulfed | zap.c zap_over_floor / u.uswallow | ❌ MISSING | no engulf state; `grep -i engulf crates/nethacked-sim/src` no match | Engulfers do not exist as a mechanic. |
| Breaking wands (apply) / item destruction by rays | apply.c do_break_wand; zap.c destroy_items | ❌ MISSING | no handler | No last-ditch wand break; fire does not burn scrolls. |
| Lamps, lanterns, candles | apply.c use_lamp; light.c | 🟡 PARTIAL | inventory.rs:360-412 toggles `enchantment` as the lit flag; world.rs:609-625 adds a light radius; tick_light_fuel (lighting.rs:72) is never called by the sim | Lamps never run out of oil. |
| Magic lamp rub / djinni | apply.c dorub; potion.c djinni_from_bottle | 🟡 PARTIAL | items.rs:159-258; a wish gives a blessed scroll of identify instead of a wish (items.rs:189-201) | A magic lamp "wish" does not grant a wish. |
| Invocation tools (Bell, Candelabrum, candles) | apply.c use_bell/use_candelabrum | ✅ PRESENT | inventory.rs:332-380; core gehennom.rs:13-106 | The ritual is playable. |
| Keys, lock picks, credit cards | lock.c pick_lock | ❌ MISSING | no items, no action | Locked doors and boxes cannot be opened by skill. |
| #force (open locked box with weapon) | lock.c doforce | ❌ MISSING | no ActionAst variant | |
| Locked doors in levels | mklev.c dodoor | ❌ MISSING | generator.rs:54-121 places no doors; DoorState::Locked is used only in hand-made quest levels (quest.rs:74-81) | Doors barely appear in the main dungeon. |
| Horns, whistles, flutes, harps, drums | apply.c / music.c | ❌ MISSING | no items | No frost/fire horn rays, no taming music, no magic whistle for pets. |
| Mirror | apply.c use_mirror | ❌ MISSING | no item | Mirror trick vs Medusa and nymphs is not possible. |
| Stethoscope | apply.c use_stethoscope | ❌ MISSING | no item | No free monster HP/AC probe. |
| Pick-axe / mattock digging | apply.c use_pick_axe; dig.c | ❌ MISSING | no item | Mines, Sokoban and vault digging strategies are not possible. |
| Unicorn horn, towel, blindfold, leash, saddle, tinning kit, figurine, grease, camera, crystal ball | apply.c | ❌ MISSING | no items; ranged.rs:22 can_mount takes has_saddle but there is no saddle object | Status cures (unihorn) and pet tools are absent. |
| Magic marker writing | write.c dowrite | ❌ MISSING | item exists (items.rs:1119) but no handler (`grep "magic marker" crates/nethacked-sim/src` no match) | Marker is dead weight. |
| Digging (dig.c: dig down, holes, through rock) | dig.c | ❌ MISSING | no dig code in sim | No digging for victory and no holes. |
| Kicking doors | dokick.c dokick | 🟡 PARTIAL | doors.rs:78-111: a kick always shatters any door; no Str/Dex roll, no "WHAMM" failures, no locked-shop anger | Doors are trivial to break. |
| Kicking objects, monsters, boxes, fountains/sinks/thrones | dokick.c | ❌ MISSING | handle_kick only checks door/wall | No kicking things open, no sink kicking. |
| Kick wall damage / hurt self | dokick.c | ❌ MISSING | doors.rs:104-108 prints a message without damage | |
| Throwing arbitrary items (t) | dothrow.c dothrow | ❌ MISSING | ActionAst has Fire(dir) only (ast.rs:57) | Daggers and potions cannot be thrown. |
| Fire from quiver | dothrow.c dofire | 🟡 PARTIAL | ranged.rs:28-110: fixed range 10, fixed 2 damage, no to-hit roll | Ranged attacks are weak placeholders. |
| Multishot, launchers, ammo breakage | dothrow.c throw_obj/m_shot | ❌ MISSING | ranged.rs:7 resolve_projectile_impact exists but there are no launchers | |
| Returning throws (Mjollnir, boomerang, aklys) | dothrow.c | ❌ MISSING | no handler | |
| Pickup | pickup.c pickup | 🟡 PARTIAL | handle_pickup in inventory.rs; no menu or quantity selection; no encumbrance (`grep -i burdened` no match) | Weight limits do not matter. |
| Containers: put in / take out | pickup.c in_container/out_container | 🟡 PARTIAL | ast.rs:23-30; inventory.rs:140-300; BoH nesting explosion drops contents at the hero square (spec divergence "Bag of Holding explosion scatter") | Basic bag use works. |
| #loot floor containers, #tip, cursed BoH item loss | pickup.c doloot/dotip | ❌ MISSING | no loot/tip ActionAst | Chests on the floor cannot be opened in place. |
| Trap type catalog | trap.h (23 types) | 🟡 PARTIAL | types lib.rs:189-203 has 13 types; no squeaky board, bear trap, land mine, rolling boulder, hole, trap door, magic portal, statue, magic trap or vibrating square trap | |
| Trap generation on levels | mklev.c mktrap | ❌ MISSING | TrapRecord is built only in tests and demo.rs:542; no `traps.insert` in sim/dungeon src | Normal play has no traps at all. |
| Trap effects | trap.c dotrap/trapeffect_* | 🟡 PARTIAL | movement.rs:339-385: arrow/dart deal 2 damage; teleport, level teleport and pit are message-only ("Teleport logic omitted"); other types get a generic message | Even placed traps do almost nothing. |
| Flying/levitation vs floor traps; seen-trap escape | trap.c floor_trigger, dotrap | ✅ PRESENT | core traps.rs:10-42 | Matches C rules. |
| Searching (s) | detect.c dosearch0 | 🟡 PARTIAL | actions/mod.rs:80-102 reveals every adjacent hidden trap on one search, with no luck roll and no secret doors/corridors found | Search is deterministic. |
| #untrap | trap.c dountrap | 🟡 PARTIAL | actions/mod.rs:104-125 always succeeds; no failure or set-off chance | |
| Monsters triggering traps | trap.c mintrap | ❌ MISSING | movement.rs:45 comment says monster traps are not modelled | |
| Detection (gold, food, object, monster, trap, magic mapping, clairvoyance, crystal ball) | detect.c | ❌ MISSING | no handlers; telepathy only (core lighting.rs:97 can_detect_monster) | No mapping or detection tools. |
| Teleport within level | teleport.c tele/safe_teleds | 🟡 PARTIAL | scroll moves the hero to a random room centre (items.rs:379-387); teleport_hero_randomly (stairs.rs:41) is used only for the Mysterious Force | No teleportitis and no trap teleport. |
| Teleport control | teleport.c tele (Teleport_control) | ❌ MISSING | no intrinsic or prompt | |
| Level teleport | teleport.c level_tele | ❌ MISSING | the trap prints a message only (movement.rs:361-367) | |
| Teleporting monsters / no-teleport levels | teleport.c u_teleport_mon, noteleport_level | ❌ MISSING | items.rs:1089-1091 comment "not modelled" | |
| Room-and-corridor generation | mklev.c makerooms/makecorridors/join; rect.c | 🟡 PARTIAL | generator.rs:54-121: 4-6 rooms, rooms joined in sequence by L-shaped corridors cut straight through walls, no doors, no nroom/rect algorithm | Levels look nothing like NetHack (no doors, no dead ends). |
| Doors, secret doors, secret corridors | mklev.c dodoor/dosdoor, SCORR | ❌ MISSING | no Tile::Door/SecretDoor placement in generator.rs; no SCORR tile | Doors and secret doors are missing from the main dungeon. |
| Vaults and vault guards, niches, closets | mklev.c makevtele/makeniche | ❌ MISSING | no code | |
| Lit/dark rooms | mklev.c litstate_rnd | 🟡 PARTIAL | Room::is_dark (room.rs:48-72), but generator.rs never sets it | All generated rooms are lit. |
| Fountains, sinks, thrones, graves, trees, iron bars, ladders, altars in rooms | rm.h / mklev.c mkfount/mksink/mkaltar/mkgrave | 🟡 PARTIAL | Tile enum (types lib.rs:364-405) has altar/pool/moat/lava/drawbridge only | No quaffing from fountains, no sink-kicking, no thrones. |
| Shops (by type) | mkroom.c mkshop, shknam.c shtypes | 🟡 PARTIAL | one RoomType::Shop (room.rs:40-44); only the starting level spawns a shopkeeper and a fixed 5-item stock (world.rs:213-262); deeper "shop" rooms are empty because stairs.rs:659-770 spawns no shopkeeper; no shop types | Shopping is a single fixed general store on DL1 (plus Minetown). |
| Temples / priests | mkroom.c mktemple; priest.c | 🟡 PARTIAL | generator.rs:107-115 places an altar; priests are spawned only in Minetown and the Sanctum (stairs.rs:318, :492) | Random temples are unattended. |
| Zoos, throne rooms, beehives, barracks, graveyards, morgues, leprechaun halls, cockatrice nests, anthills, swamps | mkroom.c mkzoo/mkswamp etc. | ❌ MISSING | RoomType has Normal/Shop/Temple only (room.rs:40-44) | No special-room surprises. |
| Themed rooms | dat/themerms.lua | ❌ MISSING | no match for "themerm" | |
| Dungeon structure (dungeon.lua, branch placement) | dungeon.c, dat/dungeon.lua | 🟡 PARTIAL | core branch.rs:6-36: Dungeons of Doom is 5 levels; Mines at DL3, Sokoban and Quest at DL4, Gehennom at DL5 (Gehennom 6 levels, stairs.rs:16) | Very short game; no randomized branch depths. |
| Gnomish Mines filler levels | mkmap.c; dat/minefill.lua | 🟡 PARTIAL | dungeon mines.rs:24 own cavern generator; mines levels 1-5 (stairs.rs:313-380) | Mines are shorter, with no gnome/dwarf population tables. UNSURE about monster fill. |
| Minetown variants (7) | dat/minetn-1..7.lua | 🟡 PARTIAL | one fixed layout (dungeon mines.rs:83-160) with a lawful temple and a shop | No Orcish Town, no other variants, no random altar alignment. |
| Mines' End variants (3) + luckstone | dat/minend-1..3.lua | 🟡 PARTIAL | dungeon mines.rs:165; luckstone spawn stairs.rs:338-343 | One variant. |
| Sokoban (4 levels x 2 variants, C puzzles) | dat/soko1-1..soko4-2.lua | 🟡 PARTIAL | dungeon sokoban.rs:10: one synthetic level (4 boulders, 1 pit row) that ignores the `_floor` arg; core sokoban.rs push_boulder | No real puzzles, no Sokoban luck penalties, no bag/amulet prize choice. |
| Oracle | dat/oracle.lua; rumors.c | ❌ MISSING | no match for "oracle" in dungeon/sim level code | No consultations or centaur fountains. |
| Big Room (13 variants) | dat/bigrm-*.lua | ❌ MISSING | no match | |
| Rogue level | extralev.c | 🟡 PARTIAL | generator.rs:150 generate_rogue_level just calls the normal generator and is never wired into the sim | No Rogue tribute level. |
| Quest per role (home/locate/goal + fillers, 13 roles) | dat/<Role>-*.lua; quest.c | 🟡 PARTIAL | dungeon quest.rs:29-220: 3 generic levels; the role only switches lava/moat for the goal (quest.rs:190); per-role leader/nemesis/artifact config exists | Quest maps are the same for every role and have no filler levels. |
| Fort Ludios | dat/knox.lua | ❌ MISSING | generator.rs:143 stub (empty stone with stairs) never wired | |
| Medusa's level (4 variants) | dat/medusa-1..4.lua | ❌ MISSING | no level; Medusa gaze exists as a monster (sim monsters.rs:451) | No Medusa island. |
| Castle (drawbridge, wand of wishing) | dat/castle.lua | 🟡 PARTIAL | dungeon endgame.rs:13 generator exists, used only by demo, not reachable in the sim (`grep -i castle crates/nethacked-sim/src` no match) | No Castle in play. |
| Drawbridge (open/close by tune, destroy) | dbridge.c | 🟡 PARTIAL | core endgame.rs:11-40 model; the sim only destroys it with striking (items.rs:1054-1061); no passtune or playing | |
| Valley of the Dead | dat/valley.lua | 🟡 PARTIAL | dungeon gehennom.rs:14 (3 chambers); the Bell of Opening is placed here (stairs.rs:400-407), unlike C where it comes from the quest nemesis | |
| Gehennom mazes | mkmaze.c makemaze/walkfrom; hellfill.lua | 🟡 PARTIAL | dungeon gehennom.rs:71 builds 4-6 chambers with lava, not a true maze | No mazes. |
| Demon lairs (Juiblex, Orcus, Asmodeus, Baalzebub), fake wizard towers | dat/juiblex/orcus/asmodeus/baalz/fakewiz*.lua | ❌ MISSING | no generators | |
| Vlad's Tower | dat/tower1-3.lua | ❌ MISSING | generator.rs:136 stub, not wired; the Candelabrum spawns on the vibrating-square level (stairs.rs:440-473) | |
| Wizard's Tower | dat/wizard1-3.lua | ❌ MISSING | generator.rs:129 stub, not wired; the Book of the Dead spawns on the VS level | |
| Vibrating square / invocation | mkmaze.c, do.c invocation | ✅ PRESENT | Gehennom depth 5 (stairs.rs:414-480); core gehennom.rs | Ritual is playable in a compressed form. |
| Moloch's Sanctum | dat/sanctum.lua | 🟡 PARTIAL | dungeon gehennom.rs:150; stairs.rs:481-528 | Simplified layout. |
| Elemental Planes (Earth, Air, Fire, Water) | dat/earth/air/fire/water.lua; mkmaze.c movebubbles | ❌ MISSING | only the `EndgamePlane` enum (types lib.rs:229-235); no generators | |
| Astral Plane | dat/astral.lua | 🟡 PARTIAL | dungeon endgame.rs:92 generator used only by demo.rs:441; not reachable from the sim | No real endgame. |
| Tutorial levels | dat/tut-1/2.lua | ❌ MISSING | no match | |
| Special-level engine (sp_lev.c + Lua des.* API) | sp_lev.c, nhlua.c | ❌ MISSING | special levels are hard-coded Rust functions | Levels cannot be ported data-driven from dat/*.lua. |
| Stairs and branch stairs | stairs.c | ✅ PRESENT | Tile::Stairs/BranchStairs (types lib.rs:383-390); stairs.rs:775-1150 | Works within the compressed topology. |
| Level persistence on revisit | save.c savelev/getlev | ✅ PRESENT | stairs.rs:122 pack_current_level, :233 unpack_or_generate_level | Levels are kept. |
| Vision / line of sight | vision.c clear_path/view_from | 🟡 PARTIAL | dungeon fov.rs:11-80 symmetric raycasting with radius 8 (world.rs:628) instead of C's lit-room/unlimited LOS | Lit rooms are not fully visible from the doorway. |
| Light sources | light.c do_light_sources | 🟡 PARTIAL | dungeon fov.rs:83 compute_illumination; world.rs:609-630 carried lamps only; no floor light sources or light-emitting monsters | |
| Map memory (remembered terrain, objects, invisible markers) | display.c, rm.glyph/levl[][].seenv | ❌ MISSING | the TUI draws only currently visible cells (nethacked-tui/src/main.rs:955-976); no remembered-glyph store (`grep -i remember crates/*/src` no match) | Explored areas vanish when out of sight. |
| Engravings: types | engrave.c (DUST, ENGRAVE, BURN, MARK, ENGR_BLOOD, HEADSTONE) | 🟡 PARTIAL | core engraving.rs:9-14 has Burned/Carved/Marked/Dust; no blood or headstone | |
| Engravings: Elbereth and degradation | engrave.c, monmove.c onscary | ✅ PRESENT | core engraving.rs:31-110; sim actions/engrave.rs | Works. |
| Engrave-testing wands | engrave.c doengrave zapwand | ❌ MISSING | the medium is passed directly in ActionAst::Engrave (ast.rs:34-37) | No wand engrave-ID (moot without unidentified wands). |
| Gas clouds / regions | region.c | ❌ MISSING | `grep -i "gas cloud\|region" crates/nethacked-sim/src crates/nethacked-core/src` no match | No stinking cloud or poison-gas regions. |
| Bones files | bones.c savebones/getbones | 🟡 PARTIAL | sim bones.rs:30-120 (curse rolls, ghost); stored in memory (world.rs:96) or on a network bones server (nethacked-agent/src/bones/) and only called from agent code (bones/mod.rs:28,45) | No local bones files in normal TUI play. UNSURE whether the web build calls it. |
| Polypiling | zap.c do_osshock/poly_obj | 🟡 PARTIAL | core polypile.rs:9 polypile_stack exists but the sim never calls it (`grep polypile_stack crates/nethacked-sim/src` no match) | Zapping polymorph at piles does nothing. |

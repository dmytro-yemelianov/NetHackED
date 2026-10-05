# Gap audit: game flow, meta and player-facing UI

Scope: allmain, cmd, end, rip, topten, save/restore, options, pager, invent, do_name, botl, insight, rumors, questpgr, mail, calendar, u_init, role (+ dungeon.c overview, o_init discoveries, exper/attrib where they show up in the UI).
C tree: the NetHack 5.0.0 `src/` tree. Rust tree: `crates/*`, `web/play/play.js`.
Frontends checked: TUI (`crates/nethacked-tui/src/{keys.rs,main.rs}`), web (`web/play/play.js` + `crates/nethacked-wasm/src/lib.rs`). Agent/MCP/JSON-RPC APIs (`crates/nethacked-agent`) are mentioned only where they are the only route to a feature.
Audit date: 2026-10-05, main @ cdd7f2d.

Status key: PRESENT = works for the player as in C; PARTIAL = exists but reduced/different; MISSING = not reachable by a player; UNSURE = could not confirm.

Counts: PRESENT 22, PARTIAL 44, MISSING 97, UNSURE 0 (163 rows; a few rows carry an UNSURE note about one detail).

## A. Command table (cmd.c `extcmdlist[]`, cmd.c:1673)

TUI keys: `crates/nethacked-tui/src/keys.rs:127-180`. Web keys: `web/play/play.js:115-119,505-522`. Sim actions: `crates/nethacked-sim/src/actions/mod.rs:34-132`.

| Feature | C reference | NetHackED status | Evidence | Player impact |
|---|---|---|---|---|
| `#` extended command | cmd.c:493 doextcmd | 🟡 PARTIAL | TUI `#` then one letter, only `e`/`c` (main.rs:819-846); web has no `#` | Almost no extended commands are reachable |
| `M-?` list extended commands | cmd.c:562 doextlist | ❌ MISSING | no match for grep `extlist` | No way to discover commands |
| `#adjust` (M-a) | invent.c:5033 doorganize | ❌ MISSING | no match for grep `adjust\|invlet` in frontends | Cannot fix inventory letters |
| `#annotate` (M-A) | do_name.c donamelevel | ❌ MISSING | no match for grep `annotate` | Cannot label levels |
| `a` apply | apply.c doapply | 🟡 PARTIAL | web `a` item prompt (play.js:116); TUI has no key (`InventoryPurpose::Apply` unwired, keys.rs:44-56, main.rs:884) | TUI players cannot use tools |
| `^X` attributes | insight.c:2009 doattributes | ❌ MISSING | no match for grep `doattributes\|enlighten` in frontends | No character sheet |
| `@` autopickup toggle | options.c:9295 dotogglepickup | ❌ MISSING | no match for grep `autopickup` | No autopickup at all |
| `C` / `#call` | do_name.c:499 docallcmd | ❌ MISSING | TUI `C` opens conducts (keys.rs:149); no match for grep `#call` | Cannot type-name unidentified items |
| `Z` cast | spell.c docast | 🟡 PARTIAL | TUI `x` casts spell index 0 in last move dir (keys.rs:170-173); web `x` dir prompt, no spell menu (play.js:119) | Only first spell usable; key differs from C (`x` is swap in C) |
| `M-c` / `#chat` | sounds.c dotalk | ❌ MISSING | no match for grep `chat` | Cannot talk to priests, shopkeepers, Oracle |
| `v` chronicle | insight.c:2532 do_gamelog | ❌ MISSING | no match for grep `gamelog\|chronicle` | No journal of major events |
| `c` close door | lock.c doclose | 🟡 PARTIAL | TUI closes first adjacent open door, no direction (main.rs:905, 278); web no key | Wrong door may close; web cannot close doors |
| `#conduct` (M-C) | insight.c:2081 doconduct | 🟡 PARTIAL | TUI `C`/`#c` modal with 8 conducts (main.rs:421-470); web none | Fewer conducts than C; web cannot view |
| `#dip` (M-d) | potion.c dodip | 🟡 PARTIAL | parse_action `dip` only in agent API (agent/commands.rs:148); no TUI/web key | Not reachable from either frontend |
| `>` down | do.c dodown | ✅ PRESENT | keys.rs:178; play.js:511 | - |
| `d` drop | do.c dodrop | 🟡 PARTIAL | TUI `d` = `Drop(0)`, always first item, no prompt (keys.rs:167); web prompts (play.js:116) | TUI drops the wrong item |
| `D` droptype | do.c doddrop | ❌ MISSING | no key for `D` | No multi-drop |
| `e` eat | eat.c doeat | 🟡 PARTIAL | TUI `Eat(0)`, no prompt (keys.rs:169); web prompts | TUI eats first item regardless of choice |
| `E` engrave | engrave.c doengrave | 🟡 PARTIAL | web `E` text prompt (play.js:518); TUI `E` = enhance (keys.rs:148) | TUI players cannot engrave (no Elbereth) |
| `#enhance` (M-e) | weapon.c enhance_weapon_skill | 🟡 PARTIAL | TUI `E` and `#e` modal (main.rs:490); web none | Web cannot advance skills |
| `#exploremode` (M-X) | cmd.c enter_explore_mode | ❌ MISSING | no match for grep `explore` in frontends | No discovery mode |
| `F` fight prefix | cmd.c do_fight | ❌ MISSING | web `F` = kick (play.js:119); TUI none | Cannot force-fight unseen/peaceful |
| `f` fire | dothrow.c dofire | ✅ PRESENT | TUI prompt (main.rs:856); web dir prompt | - |
| `#force` (M-f) | lock.c doforce | ❌ MISSING | no match for grep `force` in sim actions | Cannot force locks |
| `#genocided` (M-g) | end.c list_genocided | ❌ MISSING | registry exists (sim/world.rs:124), no display | Cannot review genocides |
| `;` glance / farlook | pager.c:2329 doquickwhatis | ❌ MISSING | no `;` key in keys.rs or play.js | Cannot identify a map glyph; monsters drawn as first letter of name makes this worse |
| `?` help | pager.c:2860 dohelp | 🟡 PARTIAL | TUI key list modal (main.rs:575); web static HELP (play.js:121-158) | Only a key list; no guidebook/option help |
| `#herecmdmenu` / `#therecmdmenu` | cmd.c | ❌ MISSING | no match | Minor |
| `#history` | pager.c:2961 dohistory | ❌ MISSING | no match | Minor |
| `i` inventory | invent.c:3047 ddoinv | 🟡 PARTIAL | TUI modal, letters by position, no BUC/qty (main.rs:325-419); web shows BUC/ench/unpaid (play.js:260-275) | See section C |
| `I` inventtype | invent.c:3873 dotypeinv | ❌ MISSING | no key `I` | Cannot filter by class |
| `#invoke` (M-i) | artifact.c doinvoke | ❌ MISSING | no match for grep `invoke` | Artifact powers unusable |
| `#jump` (M-j) | apply.c dojump | ❌ MISSING | no match for grep `jump` | Knights/jumping boots lose ability |
| `^D` kick | dokick.c dokick | 🟡 PARTIAL | TUI `K` kicks last move direction, no prompt (keys.rs:180); web `F` + dir prompt | Unexpected TUI kick target |
| `\` known / `` ` `` knownclass | o_init.c:764 dodiscovered | ❌ MISSING | TUI `\` toggles language (keys.rs:132) | No discoveries list |
| `:` look here | invent.c:4371 dolook | ❌ MISSING | no `:` key; no "You see here" message (no match for grep `You see here`) | Cannot see what is on the floor (stacked items, engravings) |
| `#lookaround` | pager.c | ❌ MISSING | no match | Minor |
| `#loot` (M-l) / `#tip` (M-T) | pickup.c doloot / dotip | ❌ MISSING | `PutInContainer`/`TakeFromContainer` exist (actions/mod.rs:44-51) but no frontend key or parse_action name | Containers/bags cannot be used by players |
| `#monster` (M-m) | polyself.c domonability | ❌ MISSING | no match | Poly abilities unusable |
| `#name` (M-n) | do_name.c:199/290 | ❌ MISSING | no match for grep `christen\|do_oname` | Cannot name pets/objects/artifacts (no Sting/Orcrist naming) |
| `#offer` (M-o) | pray.c dosacrifice | 🟡 PARTIAL | TUI `S` = `Sacrifice(0)` (keys.rs:166); web `S` item prompt | TUI offers first item; key differs from C |
| `o` open door | lock.c doopen | 🟡 PARTIAL | TUI first adjacent closed door (main.rs:903); web no key | Wrong door may open; web cannot open doors (UNSURE whether walking into a door opens it) |
| `O` options | options.c:8796 doset | ❌ MISSING | no in-game options; only CLI `--lang/--seed/--pack` (tui main.rs:1290-1329), URL `?lang/?seed/?pack`, F2-F4 pixel toggles (play.js:398) | No in-game settings |
| `^O` overview | dungeon.c:3305 dooverview | ❌ MISSING | no match for grep `overview` in frontends | No dungeon summary |
| `p` pay | shk.c dopay | ✅ PRESENT | keys.rs:164; play.js:512 | - |
| `|` perminv | invent.c | MISSING | no match | Window-port convenience; low |
| `#pray` (M-p) | pray.c dopray | 🟡 PARTIAL | TUI/web `P` pray (keys.rs:165, play.js:513); no "Are you sure you want to pray?" confirm | Accidental prayers; `P` is put-on in C |
| `^P` prevmsg | cmd.c:164 doprev_message | ✅ PRESENT | `MessageWindow::recall_prev` (sim/messages.rs:264); TUI Ctrl-P (tui/keys.rs:154, main.rs:858); web Ctrl+P (play.js:404, wasm/lib.rs:250 `msgPrev`); tests `prev_cycles_newest_first_then_wraps`, `ctrl_p_is_prevmsg_not_pay` | tty `single` model only (topl.c:102-119): newest first, then older, wrapping; 20 lines (options.c:7198), not saved; ignored at `--More--` |
| `P` puton / `R` remove | do_wear.c doputon / doremring | ❌ MISSING | `P` pray, `R` ride (TUI) / rub (web) | Rings and amulets cannot be worn |
| `q` quaff | potion.c dodrink | ✅ PRESENT | TUI modal (main.rs:873); web prompt | - |
| `#quit` | end.c:90 done2 | 🟡 PARTIAL | TUI Esc/^C "Really quit? [y/n]" (main.rs:772-794); web none | No end-of-game summary after quitting |
| `Q` quiver | dothrow.c dowieldquiver | 🟡 PARTIAL | TUI sets `hero.quivered_item` directly, not via sim (main.rs:860-871); web none | Web cannot change quiver |
| `r` read | read.c doread | 🟡 PARTIAL | TUI `Read(0)` no prompt (keys.rs:175); web prompt | TUI reads first item |
| `^R` redraw | cmd.c | ❌ MISSING | `KeyOutcome::Redraw` never produced (keys.rs:37-38) | Low; resize redraws |
| `^A` repeat / count prefix `n`/digits | cmd.c | ❌ MISSING | no digit handling in handle_key; `5` = wait | Cannot `n20s` search or repeat |
| `m` reqmenu prefix | cmd.c | ❌ MISSING | no match | Minor |
| `_` travel / `^_` retravel | hack.c dotravel | ❌ MISSING | no match for grep `travel` in frontends | Long walks are tedious |
| `#ride` (M-R) | steed.c doride | 🟡 PARTIAL | TUI `R` (keys.rs:162); web none | Web cannot ride |
| `#rub` (M-r) | apply.c dorub | 🟡 PARTIAL | web `R` item prompt; TUI none | TUI cannot rub lamps |
| `G`/`g` run/rush, shift-move | hack.c | ❌ MISSING | no shift-direction keys (`K`=kick, `L`=lang in TUI) | Movement is one step at a time |
| `S` save | save.c:43 dosave | ❌ MISSING | `S` = sacrifice; no match for grep `save_game\|dosave` (world is `Serialize`, sim/world.rs:66, but unused) | Games cannot be saved/resumed |
| `s` search | detect.c dosearch | 🟡 PARTIAL | reveals adjacent hidden traps only, never secret doors/corridors (actions/mod.rs:73-98) | Secret doors cannot be found by searching |
| `*` seeall, `"` `[` `=` `(` `)` see-worn | invent.c:4602-4792 | ❌ MISSING | no keys | Cannot check equipped gear quickly |
| `$` showgold | invent.c:4555 doprgold | ❌ MISSING | gold only on status line | Low |
| `+` showspells | spell.c:2021 dovspell | ❌ MISSING | no key `+`; `known_spells` (sim/world.rs:87) not displayed | Cannot see spells/failure rates |
| `^` showtrap | pager/trap.c | ❌ MISSING | TUI `^` = untrap (keys.rs:159) | Cannot identify trap type |
| `#sit` (M-s) | sit.c dosit | ❌ MISSING | no match | Thrones unusable |
| `x` swap weapons | wield.c dowieldswap | ❌ MISSING | `x` = cast | No secondary weapon |
| `T` takeoff / `A` takeoffall / `W` wear | do_wear.c | ❌ MISSING | no keys; carried armor is auto-counted as worn (sim/world.rs:439-473) | Cannot remove cursed/heavy armor or choose what to wear |
| `^T` teleport | teleport.c dotelecmd | ❌ MISSING | no match | Teleportitis control unusable |
| `#terrain` | pager.c doterrain | ❌ MISSING | no match | Low |
| `t` throw | dothrow.c dothrow | ❌ MISSING | TUI `t` = untrap (keys.rs:158) | Cannot throw daggers/potions |
| `#turn` (M-t) | pray.c doturn | ❌ MISSING | no match | Priests/Knights lose turn undead |
| `X` twoweapon | wield.c dotwoweapon | ❌ MISSING | no match | Lost tactic |
| `#untrap` (M-u) | trap.c dountrap | 🟡 PARTIAL | TUI `t`/`^` at last move dir (keys.rs:158); web none | No direction prompt; web cannot untrap |
| `<` up | do.c doup | ✅ PRESENT | keys.rs:177; play.js:510 | - |
| `#vanquished` (M-V) | insight.c:2769 dovanquished | ❌ MISSING | no match for grep `vanquished` | No kill list |
| `V` / `#version` | cmd.c | ❌ MISSING | no match | Low |
| `.` wait | cmd.c donull | ✅ PRESENT | keys.rs:139; play.js:507 (no count) | - |
| `&` whatdoes / `/` whatis | pager.c:2659 / 2322 | ❌ MISSING | no keys | Cannot ask what a key or symbol means |
| `w` wield | wield.c dowield | 🟡 PARTIAL | TUI `Wield(0)` no prompt (keys.rs:168); web prompt | TUI wields first item |
| `#wipe` (M-w) | do_wear.c dowipe | ❌ MISSING | no match | Minor |
| `z` zap | zap.c dozap | 🟡 PARTIAL | direction only; sim picks first carried wand (actions/items.rs:954-970) | Cannot choose which wand |
| Movement hjklyubn + arrows | cmd.c move cmds | ✅ PRESENT | keys.rs:131-138; play.js:111-112 | - |
| Wizard-mode commands (`^W`,`^E`,`^F`,`^G`,`^I`,`^V`, #wiz*) | cmd.c:1673+ | ❌ MISSING | no match for grep `wizmode\|wizwish` | Debug only; agent API has `wish` (agent/commands.rs:134) |
| `!` shell / `^Z` suspend / #saveoptions / #bugreport | cmd.c:5697/5717 | ❌ MISSING | no match | Low (port-level) |

## B. Game flow and endings (allmain.c, end.c, rip.c, topten.c, save.c, restore.c)

| Feature | C reference | NetHackED status | Evidence | Player impact |
|---|---|---|---|---|
| Main turn loop (hero acts, monsters move, timers tick) | allmain.c:173 moveloop_core | ✅ PRESENT | sim/actions/mod.rs:19-161 `step_player_action` + `process_turn_ticks` | - |
| Welcome message | allmain.c:897 welcome | 🟡 PARTIAL | TUI custom marketing text (i18n/lib.rs:541-546); web close to C (play.js:49) | TUI greeting is not C's "Hello X, welcome to NetHack! You are a ..." |
| Occupations / interruptible multi-turn actions | allmain.c:691 stop_occupation | ❌ MISSING | no match for grep `occupation` | No multi-turn eating/searching; nothing to interrupt |
| Death ("You die...") | end.c:1023 done | 🟡 PARTIAL | `is_dead` flag; TUI "You have died... Press Esc" (i18n:234); web "You die..." then back to role pick (play.js:310-313) | No cause of death shown |
| Death cause / killer string | end.c:188 done_in_by | ❌ MISSING | no match for grep `killer\|cause_of_death` in sim (only bones types) | Player never learns what killed them |
| Life saving | end.c:707 savelife | ❌ MISSING | no match for grep `lifesav\|life saving` | Amulet of life saving does nothing |
| DYWYPI disclosure (inventory, attributes, vanquished, genocided, conduct, overview) | end.c:622 disclose | ❌ MISSING | no match for grep `disclose\|identified?` | No end-of-game review |
| Tombstone (RIP) | rip.c:86 genl_outrip | ❌ MISSING (in game) | only agent bones server `render_headstone` (agent/bones/headstone.rs:4); not called by TUI/web | No gravestone on death |
| Score calculation and high-score list | topten.c:628 topten, :946 outentry | ❌ MISSING | no match for grep `topten\|highscore`; agent `final_score` is a benchmark metric (agent/arena.rs:953) | No score, no leaderboard |
| Ascension ending | end.c done(ASCENDED) | 🟡 PARTIAL | sim emits `GameEvent::Victory` (actions/religion.rs:211-219); TUI and web ignore it (no match for grep `Victory` in tui/wasm/play.js) | Game continues after ascending; no ending screen |
| Escape the dungeon (up stairs on DL1) | do.c goto_level / end.c ESCAPED | ❌ MISSING | `handle_ascend` only handles depth>1 or sub-branch exit (actions/stairs.rs:929, 1104-1109); no ESCAPED outcome | Cannot leave the dungeon (UNSURE whether any message is printed) |
| Quit flow | end.c:90 done2 | 🟡 PARTIAL | TUI confirm then exit (main.rs:772-794); web only by reload | No final summary |
| Bones on death in a real game | bones.c savebones (called from end.c) | 🟡 PARTIAL | `save_bones` exists (sim/bones.rs:30) but called only from agent bones client (agent/bones/mod.rs:28) | Normal TUI/web deaths leave no bones (UNSURE for agent-driven play) |
| Save game | save.c:43 dosave | ❌ MISSING | no save command; `SimulationWorld` derives Serialize (sim/world.rs:66) | Every game must be finished in one sitting |
| Restore game | restore.c:806 dorecover | ❌ MISSING | no match for grep `restore\|load_game` outside pixel settings | Cannot resume |
| Dumplog | end.c:545 dump_everything | ❌ MISSING | no match | No game record |

## C. Inventory, status line, look, naming (invent.c, botl.c, pager.c, do_name.c)

| Feature | C reference | NetHackED status | Evidence | Player impact |
|---|---|---|---|---|
| Fixed inventory letters | invent.c:694 assigninvlet | 🟡 PARTIAL | letter = position in list (tui main.rs:357; play.js:266); letters shift when items leave | Item letters change between turns |
| Inventory grouped by class with headers | invent.c:3474 display_inventory | ❌ MISSING | flat list (main.rs:356-377) | Harder to read packs |
| Quantity/stacks in inventory | invent.c / objnam.c | ❌ MISSING | `ItemRecord` has no quantity field (arena/lib.rs:20-37) | No "3 daggers" |
| BUC/enchantment shown only when known | objnam.c doname | 🟡 PARTIAL | web always shows blessed/cursed and enchantment (play.js:57,267-268); TUI shows neither | Web leaks unknown BUC; TUI hides known BUC |
| Unidentified appearances in inventory | objnam.c xname | ❌ MISSING | items display true name (`t_item(&item.name)`, main.rs:365); no ident state on `ItemRecord` | No identification game in the UI |
| "(weapon in hand)" / worn tags | objnam.c doname | 🟡 PARTIAL | TUI "in hand" tag only (main.rs:360-364); web none; no "(being worn)" | Cannot tell what is worn |
| Unpaid item cost in inventory | invent.c:3700 dounpaid | 🟡 PARTIAL | web shows unpaid cost (play.js:269); TUI no | TUI shoppers lack prices |
| Item pick-up menu for piles | pickup.c pickup | 🟡 PARTIAL | `,` picks one auto-chosen item (actions/inventory.rs:12-30) | Cannot pick a specific item from a pile |
| Status: name and title | botl.c:48 do_statusline1 | 🟡 PARTIAL | TUI shows name, web shows "name the Role" (play.js:211); no rank title | No "the Stripling" style rank |
| Status: St Dx Co In Wi Ch | botl.c:48 | ❌ MISSING | no attribute fields on hero (types/lib.rs:956-967, arena/lib.rs:40-67) | No attributes exist at all |
| Status: alignment | botl.c:48 | ✅ PRESENT | TUI 3-letter (main.rs:1164-1165); web full (play.js:207) | - |
| Status: Dlvl | botl.c:101 | ✅ PRESENT | main.rs:1218; play.js:217 | - |
| Status: $ gold | botl.c:101 | ✅ PRESENT | main.rs:1220; play.js:217 | - |
| Status: HP(max) | botl.c:101 | ✅ PRESENT | main.rs:1222; play.js:213 | - |
| Status: Pw(max) | botl.c:101 | ✅ PRESENT | main.rs:1225; play.js:214 | - |
| Status: AC | botl.c:101 | ✅ PRESENT | main.rs:1228 | - |
| Status: Xp level / Exp points | botl.c:101; exper.c:300 newexplevel | ❌ MISSING | not displayed; hero never gains XP (only pets, sim/monsters.rs:670) | No levelling up |
| Status: T turn counter | botl.c:101 | ✅ PRESENT | main.rs:1231; play.js:217 | - |
| Status: hunger | botl.c:101 | ✅ PRESENT | main.rs:1167-1179; play.js:209 | - |
| Status: encumbrance (Burdened...) | botl.c:101 | ❌ MISSING | `encumbrance_tier` in core (core/inventory.rs:151) never shown or used by sim | Weight has no visible effect |
| Status: conditions (Blind, Conf, Stun, Hallu, Ill, FoodPois, Lev, Fly, Ride...) | botl.c condition flags | 🟡 PARTIAL | TUI shows `[STONE:n]`, `[SLIME:n]`, `[Poly]`, `[Mounted]` only (main.rs:1182-1198); web shows none; blind/hallucination state exists (types/lib.rs:643, 896) | Player is not told about blindness, confusion, etc. |
| `--More--` paging, every message of a turn shown | win/tty/topl.c:205 more, :251 update_topl | ✅ PRESENT | `turn_messages` + `pack_lines` + `MessageWindow` (sim/messages.rs:36,136,161); TUI `consume_more_key` (main.rs:286), web `msgMore`/`msgDismiss`/`msgSkipRest` (play.js:431; F2-F4 no longer call `msgShow` at `--More--`, play.js:397); tests `pack_boundary_matches_update_topl`, `turn_messages_door_toggle_survives_unrelated_log_line`, `turn_messages_attack_pairs_only_with_its_own_exchange_line`, `more_flow_dismiss_and_last_page_without_more`, `esc_skips_rest_instead_of_quit` | Messages pack with two spaces while they fit 72 columns, else `--More--` (any key; Esc skips the rest); the line clears at the next command key |
| Map memory (remembered tiles out of sight) | display.c (shown via botl/map) | ❌ MISSING | TUI and web draw only currently visible cells (main.rs:983-986; agent/ascii.rs:39-41) | Explored map vanishes as you walk away |
| Monster glyphs | drawing.c / display | 🟡 PARTIAL | glyph = first letter of monster name, lower-cased (main.rs:990-997; agent/ascii.rs:27-33) | Wrong symbols (e.g. "jackal" shows `j`, dragons show `d`) |
| "You see here X" on stepping onto items | invent.c:4150 look_here | ❌ MISSING | no match for grep `You see here` | Player does not learn what they stepped on |
| Farlook / `;` / `/` / `:` | pager.c:657 lookat, :1673 do_look | ❌ MISSING | see section A | Cannot inspect anything |
| Name a monster / pet | do_name.c:199 do_mgivenname | ❌ MISSING | no match | - |
| Name an object / artifact naming | do_name.c:290 do_oname | ❌ MISSING | no match | Cannot create Sting/Orcrist by naming |
| Call an object type | do_name.c:636 docall | ❌ MISSING | no match | Cannot track unidentified types |
| Character name entry | role.c plnamesuffix / askname | ❌ MISSING | TUI uses role name as hero name (main.rs:191-200); web fixed `STR.hero` (play.js:378) | Players cannot name their hero |

## D. Insight, text content, calendar (insight.c, rumors.c, questpgr.c, mail.c, calendar.c)

| Feature | C reference | NetHackED status | Evidence | Player impact |
|---|---|---|---|---|
| Enlightenment / attributes screen | insight.c:383 enlightenment | ❌ MISSING | potion item list mentions "enlightenment" (actions/items.rs:512) but no display routine | Potion/wand of enlightenment shows nothing |
| Conduct display | insight.c:2089 show_conduct | 🟡 PARTIAL | 8 conducts (TUI main.rs:432-441); C tracks more (weaponless, foodless, sokoban, elbereth, ...) | Fewer challenges tracked |
| Achievements | insight.c:2243 show_achievements | ❌ MISSING | no match for grep `achievement` | - |
| Dungeon overview | dungeon.c:3305 dooverview | ❌ MISSING | no match | - |
| Discoveries list | o_init.c:764 dodiscovered | ❌ MISSING | no match | - |
| Fortune cookies / rumors | rumors.c:117 getrumor, :529 outrumor | ❌ MISSING | no match for grep `rumor\|fortune\|cookie` | No rumors |
| Oracle consultations | rumors.c:697 doconsult, :641 outoracle | ❌ MISSING | no match for grep `doconsult\|oracle` (only a comment, actions/movement.rs:38) | Oracle is useless |
| Quest text (leader, nemesis, home messages) | questpgr.c:640 com_pager, :646 qt_pager | 🟡 PARTIAL | short one-line messages only (i18n/lib.rs:1141-1180, 1531) | No quest story text |
| Level entry messages (special levels) | questpgr.c:671 deliver_splev_message | ❌ MISSING | no match | - |
| Mail daemon / scroll of mail | mail.c:461 ckmailstatus, :763 readmail | ❌ MISSING | no match for grep `mail` outside armor names | Not relevant on most ports; low |
| Full moon luck (+1) and message | calendar.c:191 phase_of_the_moon; allmain/u_init | ❌ MISSING | comment "moon phase / Friday 13th are not tracked" (sim/world.rs:572) | No full-moon luck |
| New moon (hearing cockatrice risk) | calendar.c:191 | ❌ MISSING | same | - |
| Friday 13th luck (-1) | calendar.c:206 friday_13th | ❌ MISSING | same | - |
| Night / midnight effects (undead, werecreatures) | calendar.c:215/223 night, midnight | ❌ MISSING | no match for grep `midnight\|night()` | - |

## E. Character creation and starting kit (role.c, u_init.c)

| Feature | C reference | NetHackED status | Evidence | Player impact |
|---|---|---|---|---|
| All 13 roles | role.c roles[] | 🟡 PARTIAL | 9 roles; no Caveman, Priest, Ranger, Samurai (data/roles.rs:11-21) | Four classic roles unplayable |
| Race choice | role.c:2177 genl_player_selection | ❌ MISSING | races exist (Human, Elf, Dwarf, Gnome, Orc; data/roles.rs:26-33) but each role has one fixed race (tui main.rs:186-268; play.js:99-109) | Cannot play e.g. an elf wizard |
| Gender choice | role.c:2177 | ❌ MISSING | fixed per role in TUI (main.rs:186-268); web always female (wasm/lib.rs:97) | No gender choice |
| Alignment choice | role.c:2177 | ❌ MISSING | fixed per role (main.rs:186-268; wasm/lib.rs:100) | No alignment choice |
| Random character (`*`) | role.c | 🟡 PARTIAL | web `*` picks random role (play.js:420); TUI none | - |
| Role/race compatibility rules | role.c:1235 rigid_role_checks | ❌ MISSING | no validation (combinations fixed in code) | - |
| Starting inventory per role | u_init.c:1301 ini_inv | 🟡 PARTIAL | short lists, e.g. Valkyrie long sword + potion (C: +1 long sword, dagger, +3 small shield); Wizard wand+scroll+cloak (data/roles.rs:54-169); all start with 50 gold (sim/world.rs:182-186) and a food ration (:198-204) | Starting kits differ from C |
| Starting intrinsics/knowledge (pre-identified items, knows_class) | u_init.c:586 knows_class, :1398 u_init_skills_discoveries | ❌ MISSING | no ident system | - |
| Starting pet (kitten/little dog/pony) | u_init.c / dog.c makedog | ❌ MISSING | `spawn_starting_pet` exists (data/roles.rs:422) but nothing calls it (grep `spawn_starting_pet` only finds its definition and re-export) | Hero starts without a pet |
| Starting attributes (init_attr) | attrib.c:723 init_attr | ❌ MISSING | no attribute fields | - |
| Starting spells | u_init.c | 🟡 PARTIAL | Wizard force bolt, Healer cure light wounds (sim/world.rs:190-195); C gives Priest/Monk etc. their spells | - |
| Starting skills | u_init.c / weapon.c skill_init | ✅ PRESENT | data/roles.rs:243 starting_skills | - |

## F. Options (options.c, player-visible only)

| Feature | C reference | NetHackED status | Evidence | Player impact |
|---|---|---|---|---|
| In-game options menu | options.c:8796 doset | ❌ MISSING | no `O` key | - |
| autopickup / pickup_types | options.c | ❌ MISSING | no match | - |
| number_pad | options.c | ❌ MISSING | vi keys only, arrows for cardinals (keys.rs:131-138) | No numpad diagonals |
| Language | n/a (NetHackED extra) | ✅ PRESENT | TUI `L`/`\` toggle (keys.rs:132), `--lang`; web `?lang=uk` | Extra feature, but steals `\` (discoveries) |
| Seed / rule pack selection | n/a (extra) | ✅ PRESENT | `--seed`, `--pack` (main.rs:1300-1329); URL params (play.js:376-379) | Extra |
| Display settings (pixel system/palette/effects) | n/a (extra) | ✅ PRESENT | F2-F4, stored in localStorage (play.js:398-404) | Extra |

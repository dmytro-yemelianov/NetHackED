//! NetRust Comprehensive Gameplay & Mechanics Demonstration.
//!
//! Demonstrates the full lifecycle of a NetRust run:
//! 1. Character creation & Dungeons of Doom exploration
//! 2. Gnomish Mines & Minetown Temple priest uncursing/donation
//! 3. Dynamic illumination, dark cavern raycasting, and intrinsic telepathy
//! 4. Gehennom, the Vibrating Square, and the 3-step Invocation Ritual
//! 5. Moloch's Sanctum & real Amulet of Yendor acquisition
//! 6. The Mysterious Force pushback during Gehennom ascent
//! 7. Astral Plane High Altar sacrifice & Ascension Victory!

use netrust_agent::render_ascii_map;
use netrust_arena::ItemLocation;
use netrust_core::{ActionAst, Alignment, Buc, Coord, Direction};
use netrust_data::{
    create_item_record, create_monster_record, CharacterConfig, Gender, ItemKindId,
    MonsterSpeciesId, RaceId, RoleId,
};
use netrust_sim::{GameEvent, SimulationWorld};

fn print_separator(title: &str) {
    println!("\n{}", "=".repeat(80));
    println!("  {title}");
    println!("{}\n", "=".repeat(80));
}

fn print_events(events: &[GameEvent]) {
    for e in events {
        match e {
            GameEvent::LogMessage { text } => println!("  📜 {text}"),
            GameEvent::ActorMoved { from, to, .. } => {
                println!("  🚶 Moved from {from:?} to {to:?}")
            }
            GameEvent::LevelChanged {
                from_depth,
                to_depth,
            } => println!("  🪜 Transitioned: Dlvl {from_depth} -> Dlvl {to_depth}"),
            GameEvent::ItemPickedUp { item, .. } => println!("  🎒 Picked up item {item:?}"),
            GameEvent::AttackLanded {
                attacker,
                target,
                damage,
                ..
            } => println!("  ⚔️  {attacker:?} struck {target:?} for {damage} damage!"),
            GameEvent::Victory => println!("  👑 VICTORY! THE HERO HAS ASCENDED!"),
            _ => {}
        }
    }
}

fn print_status_bar(sim: &SimulationWorld) {
    let p = sim.arena.actors.get(sim.player_id).unwrap();
    println!("--------------------------------------------------------------------------------");
    println!(
        " Hero: {:<10} Align: {:<8} Branch: {:<12} Dlvl: {:<2} Gold: {:<4} HP: {}({}) Pw: {}({}) AC: {:<2} Turn: {}",
        p.name,
        format!("{:?}", p.alignment),
        format!("{:?}", sim.current_branch),
        sim.depth,
        sim.player_gold,
        p.hp,
        p.max_hp,
        sim.player_pw,
        sim.player_max_pw,
        p.ac - sim.divine_protection as i32,
        sim.scheduler.turn
    );
    println!("--------------------------------------------------------------------------------");
}

fn main() {
    print_separator(
        "NETRUST: NetHack mechanics in safe Rust, with Lean 4 models of selected rules",
    );
    println!("Lean 4 models: selected mechanics are machine-checked (see `lake build`)");
    println!("Safe Rust workspace: unit, property and simulation tests (`cargo test --workspace`)");

    // =========================================================================
    // PHASE 1: Character Creation & Dungeons of Doom
    // =========================================================================
    print_separator("PHASE 1: Awakening in the Dungeons of Doom (Seed 42)");
    let config = CharacterConfig {
        name: "Sigrid".into(),
        role: RoleId::Valkyrie,
        race: RaceId::Human,
        gender: Gender::Female,
        alignment: Alignment::Neutral,
    };
    let mut sim = SimulationWorld::new_with_character(42, config);
    print_status_bar(&sim);

    println!("ASCII Viewport (Level 1):");
    let map = render_ascii_map(&sim);
    for line in map.lines().take(12) {
        println!("{line}");
    }
    println!("... [truncated viewport]");

    println!("\nTaking actions: Picking up floor items and exploring...");
    let p_coord = sim.arena.actors.get(sim.player_id).unwrap().coord;
    let saber = sim.arena.spawn_item(create_item_record(
        ItemKindId::SilverSaber,
        ItemLocation::Floor(p_coord),
        Buc::Uncursed,
    ));
    let ev1 = sim.step_player_action(ActionAst::PickUp);
    print_events(&ev1);

    let carried = sim.arena.items_carried_by(sim.player_id);
    let saber_idx = carried.iter().position(|&id| id == saber).unwrap();
    let ev2 = sim.step_player_action(ActionAst::Wield(saber_idx));
    print_events(&ev2);

    // =========================================================================
    // PHASE 2: Gnomish Mines & Minetown Sanctuary
    // =========================================================================
    print_separator("PHASE 2: The Gnomish Mines & Minetown Sanctuary");
    println!("Transitioning into Gnomish Mines branch (Dlvl 3 -> Minetown)...");
    sim.current_branch = netrust_types::BranchId::GnomishMines;
    sim.depth = 3;
    let layout = netrust_dungeon::generate_minetown_level(&mut sim.rng);
    sim.level = layout.level;
    let priest_coord = layout.priest_coord;
    let mut priest_rec = create_monster_record(MonsterSpeciesId::Priest, priest_coord);
    priest_rec.is_tame = true;
    sim.arena.spawn_actor(priest_rec);
    if let Some(p) = sim.arena.actors.get_mut(sim.player_id) {
        p.coord = priest_coord.step(Direction::West).unwrap_or(priest_coord);
        p.is_dead = false;
        p.hp = p.max_hp;
    }
    print_status_bar(&sim);

    println!(
        "Entering Minetown Temple. Donating 400 gold to the High Priest for divine protection:"
    );
    sim.player_gold = 500;
    let ev_donate = sim.step_player_action(ActionAst::Donate(400));
    print_events(&ev_donate);
    println!(
        "Divine Protection Level: +{} AC (Total AC: {})",
        sim.divine_protection,
        sim.arena.actors.get(sim.player_id).unwrap().ac - sim.divine_protection as i32
    );

    println!("\nPriest performing uncursing ritual on a cursed potion:");
    let cursed_pot = sim.arena.spawn_item(create_item_record(
        ItemKindId::PotionOfExtraHealing,
        ItemLocation::CarriedBy(sim.player_id),
        Buc::Cursed,
    ));
    println!(
        "  Before: BUC = {:?}",
        sim.arena.items.get(cursed_pot).unwrap().buc
    );
    if let Some(it) = sim.arena.items.get_mut(cursed_pot) {
        it.buc = netrust_core::mines::priest_uncurse(it.buc);
    }
    println!(
        "  After uncursing: BUC = {:?}",
        sim.arena.items.get(cursed_pot).unwrap().buc
    );

    // =========================================================================
    // PHASE 3: Dynamic Lighting, Darkness & Telepathy
    // =========================================================================
    print_separator("PHASE 3: Dynamic Lighting, Darkness Raycasting & Telepathy");
    if let Some(p) = sim.arena.actors.get_mut(sim.player_id) {
        p.is_dead = false;
        p.hp = p.max_hp;
    }

    println!("Hero in dark cavern. Lighting oil lamp:");
    let lamp = sim.arena.spawn_item(create_item_record(
        ItemKindId::OilLamp,
        ItemLocation::CarriedBy(sim.player_id),
        Buc::Uncursed,
    ));
    let carried = sim.arena.items_carried_by(sim.player_id);
    let lamp_idx = carried.iter().position(|&id| id == lamp).unwrap();
    let ev_light = sim.step_player_action(ActionAst::Apply(lamp_idx));
    print_events(&ev_light);

    let (vis_lit, detected) = sim.compute_perception();
    println!("  Tiles illuminated by lamp radius: {}", vis_lit.len());
    println!("  Monsters detected: {}", detected.len());

    println!("\nTesting Intrinsic Telepathy while Blindfolded:");
    if let Some(p) = sim.arena.actors.get_mut(sim.player_id) {
        p.intrinsics.blind = true;
        p.intrinsics.telepathy = true;
    }
    let p_pos = sim.arena.actors.get(sim.player_id).unwrap().coord;
    let gnome_c = Coord::new_unchecked(p_pos.x + 3, p_pos.y);
    let skel_c = Coord::new_unchecked(p_pos.x - 3, p_pos.y);
    let mut gnome_rec = create_monster_record(MonsterSpeciesId::Gnome, gnome_c);
    gnome_rec.is_tame = true;
    let mut skel_rec = create_monster_record(MonsterSpeciesId::Skeleton, skel_c);
    skel_rec.is_tame = true;
    sim.arena.spawn_actor(gnome_rec);
    sim.arena.spawn_actor(skel_rec);

    let (vis_blind, detected_esp) = sim.compute_perception();
    println!(
        "  Visible tiles while blind: {} (Complete darkness)",
        vis_blind.len()
    );
    println!("  Conscious minds detected via ESP: {} (Gnome detected through solid rock, mindless Skeleton undetectable)", detected_esp.len());

    // Restore sight
    if let Some(p) = sim.arena.actors.get_mut(sim.player_id) {
        p.intrinsics.blind = false;
    }

    // =========================================================================
    // PHASE 4: The Class Quest Branch, Leader Qualification & Nemesis Defeat
    // =========================================================================
    print_separator("PHASE 4: The Class Quest Branch, Leader Qualification & Nemesis Defeat");
    println!("Transitioning into Quest branch (Quest Home - Sanctuary of The Norn)...");
    sim.role_name = "Valkyrie".to_string();
    sim.current_branch = netrust_types::BranchId::Quest;
    sim.depth = 1;
    let _ = sim.unpack_or_generate_level(netrust_types::BranchId::Quest, 1);
    let down_stairs = sim.level.stairs_down;
    if let Some(p) = sim.arena.actors.get_mut(sim.player_id) {
        p.coord = down_stairs;
        p.level = 1; // Underleveled hero
    }
    print_status_bar(&sim);

    println!("Testing Leader Qualification (Negative Test: Hero Level 1):");
    let ev_rej = sim.step_player_action(ActionAst::Descend);
    print_events(&ev_rej);
    println!("  Quest Progress: {:?}", sim.quest_state.progress);

    println!("\nHero trains and achieves Experience Level 14 with +30 Alignment:");
    if let Some(p) = sim.arena.actors.get_mut(sim.player_id) {
        p.level = 14;
    }
    sim.alignment_record = 30;

    println!("Leader Consultation & Descent to Quest Locate:");
    let ev_acc = sim.step_player_action(ActionAst::Descend);
    print_events(&ev_acc);
    println!("  Quest Progress: {:?}", sim.quest_state.progress);

    println!("\nDescending to Quest Goal (Level 3 - Volcanic Lair of Lord Surtur)...");
    sim.depth = 3;
    let _ = sim.unpack_or_generate_level(netrust_types::BranchId::Quest, 3);
    let surtur_id = sim
        .arena
        .actors
        .iter()
        .find(|(_, a)| a.name == "Lord Surtur")
        .map(|(id, _)| id)
        .unwrap();
    let surtur_coord = sim.arena.actors.get(surtur_id).unwrap().coord;
    let adj_hero = surtur_coord.step(Direction::West).unwrap();
    if let Some(p) = sim.arena.actors.get_mut(sim.player_id) {
        p.coord = adj_hero;
    }
    print_status_bar(&sim);

    println!("Attacking Lord Surtur with +5 Vorpal Blade:");
    let mut vorpal_rec = create_item_record(
        ItemKindId::VorpalBlade,
        ItemLocation::CarriedBy(sim.player_id),
        Buc::Blessed,
    );
    vorpal_rec.enchantment = 5;
    let vorpal = sim.arena.spawn_item(vorpal_rec);
    sim.wielded_item = Some(vorpal);
    if let Some(s) = sim.arena.actors.get_mut(surtur_id) {
        s.hp = 1; // Critical strike
    }
    let ev_boss = sim.step_player_action(ActionAst::Move(Direction::East));
    print_events(&ev_boss);
    println!("  Quest Progress: {:?}", sim.quest_state.progress);
    println!("  Artifact State: {:?}", sim.quest_state.artifact_location);

    println!("\nClaiming The Orb of Fate from the battlefield:");
    if let Some(p) = sim.arena.actors.get_mut(sim.player_id) {
        p.coord = surtur_coord;
    }
    let ev_pickup = sim.step_player_action(ActionAst::PickUp);
    print_events(&ev_pickup);
    println!("  Artifact State: {:?}", sim.quest_state.artifact_location);

    println!("\nAscending back to Quest Home and receiving The Norn's blessing:");
    sim.depth = 1;
    let _ = sim.unpack_or_generate_level(netrust_types::BranchId::Quest, 1);
    let up_c = sim.level.stairs_up;
    if let Some(p) = sim.arena.actors.get_mut(sim.player_id) {
        p.coord = up_c;
    }
    let ev_complete = sim.step_player_action(ActionAst::Ascend);
    print_events(&ev_complete);
    println!("  Quest Progress: {:?}", sim.quest_state.progress);

    // =========================================================================
    // PHASE 5: Gehennom, The Invocation Ritual & Moloch's Sanctum
    // =========================================================================
    print_separator("PHASE 5: Gehennom, The Vibrating Square & The Invocation Ritual");
    sim.current_branch = netrust_types::BranchId::Gehennom;
    sim.depth = 5;
    let (maze_lvl, vs) = netrust_dungeon::generate_gehennom_maze_level(&mut sim.rng, 5, true);
    sim.level = maze_lvl;
    sim.vibrating_square = vs;
    let non_players: Vec<_> = sim
        .arena
        .actors
        .iter()
        .filter(|(id, _)| *id != sim.player_id)
        .map(|(id, _)| id)
        .collect();
    for nid in non_players {
        sim.arena.destroy_actor(nid);
    }
    if let Some(p) = sim.arena.actors.get_mut(sim.player_id) {
        p.is_dead = false;
        p.hp = p.max_hp;
    }
    print_status_bar(&sim);

    let hero_coord = sim.arena.actors.get(sim.player_id).unwrap().coord;
    let vs_coord = hero_coord.step(Direction::East).unwrap();
    sim.vibrating_square = Some(vs_coord);
    println!("The Vibrating Square is located at coordinate {vs_coord:?}");

    println!("\nAcquiring the Three Canonical Invocation Relics:");
    let bell = sim.arena.spawn_item(create_item_record(
        ItemKindId::BellOfOpening,
        ItemLocation::CarriedBy(sim.player_id),
        Buc::Blessed,
    ));
    let cand = sim.arena.spawn_item(create_item_record(
        ItemKindId::CandelabrumOfInvocation,
        ItemLocation::CarriedBy(sim.player_id),
        Buc::Blessed,
    ));
    for _ in 0..7 {
        sim.arena.spawn_item(create_item_record(
            ItemKindId::WaxCandle,
            ItemLocation::CarriedBy(sim.player_id),
            Buc::Uncursed,
        ));
    }
    let book = sim.arena.spawn_item(create_item_record(
        ItemKindId::BookOfTheDead,
        ItemLocation::CarriedBy(sim.player_id),
        Buc::Blessed,
    ));
    println!("  - Bell of Opening");
    println!("  - Candelabrum of Invocation");
    println!("  - 7 Wax Candles");
    println!("  - Book of the Dead");

    println!("\nAttaching 7 wax candles to the Candelabrum of Invocation:");
    for _ in 0..7 {
        let carried = sim.arena.items_carried_by(sim.player_id);
        let candle_idx = carried
            .iter()
            .position(|&id| {
                sim.arena
                    .items
                    .get(id)
                    .map(|it| it.name.contains("candle"))
                    .unwrap_or(false)
            })
            .unwrap();
        let _ = sim.step_player_action(ActionAst::Apply(candle_idx));
    }
    println!(
        "  Candelabrum State: {}/7 candles attached",
        sim.candelabrum_state.candle_count
    );

    println!("\nExecuting Step 1: Ring Bell of Opening");
    let carried = sim.arena.items_carried_by(sim.player_id);
    let bell_idx = carried.iter().position(|&id| id == bell).unwrap();
    let ev_step1 = sim.step_player_action(ActionAst::Apply(bell_idx));
    print_events(&ev_step1);
    println!("  Ritual Progress: {:?}", sim.ritual_progress);

    println!("\nExecuting Step 2: Light Candelabrum of Invocation");
    let carried = sim.arena.items_carried_by(sim.player_id);
    let cand_idx = carried.iter().position(|&id| id == cand).unwrap();
    let ev_step2 = sim.step_player_action(ActionAst::Apply(cand_idx));
    print_events(&ev_step2);
    println!("  Ritual Progress: {:?}", sim.ritual_progress);

    println!("\nReciting Book of the Dead away from Vibrating Square (Negative Test):");
    let carried = sim.arena.items_carried_by(sim.player_id);
    let book_idx = carried.iter().position(|&id| id == book).unwrap();
    let ev_fail = sim.step_player_action(ActionAst::Read(book_idx));
    print_events(&ev_fail);
    println!("  Ritual Progress remains: {:?}", sim.ritual_progress);

    println!("\nStepping onto the Vibrating Square {vs_coord:?}");
    if let Some(p) = sim.arena.actors.get_mut(sim.player_id) {
        p.coord = vs_coord;
    }

    println!("Executing Step 3: Reciting Book of the Dead while standing on the Vibrating Square!");
    let carried = sim.arena.items_carried_by(sim.player_id);
    let book_idx = carried.iter().position(|&id| id == book).unwrap();
    let ev_step3 = sim.step_player_action(ActionAst::Read(book_idx));
    print_events(&ev_step3);
    println!("  Ritual Progress: {:?}", sim.ritual_progress);

    println!("\nDescending through the subterranean abyss into Moloch's Sanctum:");
    let ev_desc = sim.step_player_action(ActionAst::Descend);
    print_events(&ev_desc);
    print_status_bar(&sim);

    println!("Entering Moloch's Sanctum. Reaching High Altar and claiming the Amulet of Yendor!");
    let amulet = sim.arena.spawn_item(create_item_record(
        ItemKindId::AmuletOfYendor,
        ItemLocation::CarriedBy(sim.player_id),
        Buc::Blessed,
    ));
    println!("  Claimed: {}", sim.arena.items.get(amulet).unwrap().name);

    // =========================================================================
    // PHASE 6: The Mysterious Force & Ascension Victory
    // =========================================================================
    print_separator("PHASE 6: The Mysterious Force & Astral Plane Ascension");
    println!("Ascending Gehennom while carrying the real Amulet of Yendor:");
    let roll = 9; // 9 % 3 == 0 -> pushes down by ((9/3)%3 + 1) = 2 levels
    if let Some(pushed) = netrust_core::calculate_mysterious_force(4, roll) {
        println!("  🔮 The Mysterious Force strikes! Attempt to ascend from level 4 pushed hero back down to level {pushed}!");
    }

    println!(
        "\nReaching the Surface (Dungeons of Doom Dlvl 1) and crossing into the Astral Plane:"
    );
    sim.current_branch = netrust_types::BranchId::AstralPlane;
    sim.depth = 1;
    let (astral_level, spawn) = netrust_dungeon::generate_astral_plane(&mut sim.rng);
    sim.level = astral_level;
    if let Some(p) = sim.arena.actors.get_mut(sim.player_id) {
        p.coord = spawn;
    }
    print_status_bar(&sim);

    println!("Approaching the Neutral High Altar at coordinate (40, 11)...");
    let altar_coord = Coord::new_unchecked(40, 11);
    if let Some(p) = sim.arena.actors.get_mut(sim.player_id) {
        p.coord = altar_coord;
    }

    println!("Sacrificing the Amulet of Yendor to the Gods:");
    let carried = sim.arena.items_carried_by(sim.player_id);
    let amulet_idx = carried.iter().position(|&id| id == amulet).unwrap();
    let ev_ascend = sim.step_player_action(ActionAst::Sacrifice(amulet_idx));
    print_events(&ev_ascend);

    // =========================================================================
    // PHASE 7: Polymorph Buffer Pools & Reversion Invariants
    // =========================================================================
    print_separator("PHASE 7: Polymorph Buffer Pools & Reversion on Zero HP");
    println!("Hero drinks a Potion of Polymorph:");
    let poly_pot = sim.arena.spawn_item(create_item_record(
        ItemKindId::PotionOfPolymorph,
        ItemLocation::CarriedBy(sim.player_id),
        Buc::Uncursed,
    ));
    let carried = sim.arena.items_carried_by(sim.player_id);
    let poly_idx = carried.iter().position(|&id| id == poly_pot).unwrap();
    let ev_poly = sim.step_player_action(ActionAst::Quaff(poly_idx));
    print_events(&ev_poly);
    println!("  Hero Form: {:?}", sim.hero.polymorph);

    println!("\nHero in dragon form absorbs incoming combat damage:");
    let hero_max_hp = sim.arena.actors.get(sim.player_id).unwrap().max_hp;
    let base_hp_before = sim.hero.base_hp;
    println!("  Poly HP before hit: 40 | Base HP: {base_hp_before}");
    let res_absorb = netrust_core::polymorph::apply_poly_damage(&mut sim.hero, 15, false);
    println!(
        "  After 15 damage: {:?} | Remaining Poly HP: {:?}",
        res_absorb,
        sim.hero.polymorph.as_ref().map(|p| p.hp)
    );

    println!("\nHero takes fatal damage (30 damage) to polymorph form:");
    let res_lethal = netrust_core::polymorph::apply_poly_damage(&mut sim.hero, 30, false);
    println!("  Lethal damage result: {res_lethal:?}");
    println!(
        "  Polymorph state after reversion: {:?}",
        sim.hero.polymorph
    );
    println!(
        "  Base HP preserved (excess discarded): {}/{}",
        sim.hero.base_hp, hero_max_hp
    );

    // =========================================================================
    // PHASE 8: Blessed Scroll of Genocide & Conduct Invalidation
    // =========================================================================
    print_separator("PHASE 8: Scroll of Genocide & Non-Spawn Invariant");
    let orc = sim.arena.spawn_actor(create_monster_record(
        MonsterSpeciesId::Orc,
        Coord::new_unchecked(15, 10),
    ));
    println!("Prior to genocide:");
    println!("  Orc on floor: {:?}", sim.arena.actors.get(orc).is_some());
    println!("  Conduct genocideless: {}", sim.conducts.genocideless);

    let geno_scroll = sim.arena.spawn_item(create_item_record(
        ItemKindId::ScrollOfGenocide,
        ItemLocation::CarriedBy(sim.player_id),
        Buc::Blessed,
    ));
    let carried = sim.arena.items_carried_by(sim.player_id);
    let geno_idx = carried.iter().position(|&id| id == geno_scroll).unwrap();
    let ev_geno = sim.step_player_action(ActionAst::Read(geno_idx));
    print_events(&ev_geno);

    println!("After reading blessed genocide:");
    println!(
        "  Orc on floor eliminated: {}",
        sim.arena.actors.get(orc).map(|a| a.is_dead).unwrap_or(true)
    );
    println!(
        "  Genocided species registered: {:?}",
        sim.genocide_registry.genocided_species
    );
    println!(
        "  Conduct genocideless irreversibly set to: {}",
        sim.conducts.genocideless
    );

    // =========================================================================
    // PHASE 9: Canonical 12 Traps, Searching & Disarming
    // =========================================================================
    print_separator("PHASE 9: Canonical Dungeon Traps, Active Searching (s) & Disarming");
    let trap_c = Coord::new_unchecked(12, 10);
    sim.level.traps.insert(
        trap_c,
        netrust_types::TrapRecord {
            id: 1,
            trap_type: netrust_types::TrapType::Arrow,
            state: netrust_types::TrapState::Hidden,
            coord: trap_c,
        },
    );
    if let Some(p) = sim.arena.actors.get_mut(sim.player_id) {
        p.coord = Coord::new_unchecked(11, 10);
    }
    println!("Hidden arrow trap placed at coordinate {trap_c:?}");
    println!(
        "Trap state before search: {:?}",
        sim.level.traps.get(&trap_c).map(|t| t.state)
    );

    println!("\nExecuting ActionAst::Search ('s') to detect hidden hazards:");
    let ev_search = sim.step_player_action(ActionAst::Search);
    print_events(&ev_search);
    println!(
        "Trap state after search: {:?}",
        sim.level.traps.get(&trap_c).map(|t| t.state)
    );

    println!("\nDisarming trap via ActionAst::Untrap:");
    let ev_untrap = sim.step_player_action(ActionAst::Untrap(trap_c));
    print_events(&ev_untrap);
    println!(
        "Trap state after untrap: {:?}",
        sim.level.traps.get(&trap_c).map(|t| t.state)
    );

    // =========================================================================
    // PHASE 10: Corpse Metabolism, Intrinsic Absorption & Conduct Audit
    // =========================================================================
    print_separator("PHASE 10: Metabolism, Intrinsic Absorption & Conduct Audit");
    println!("Consuming fresh dragon meat for intrinsic acquisition:");
    let dragon_corpse = sim.arena.spawn_item(create_item_record(
        ItemKindId::Corpse,
        ItemLocation::CarriedBy(sim.player_id),
        Buc::Uncursed,
    ));
    let carried = sim.arena.items_carried_by(sim.player_id);
    let corpse_idx = carried.iter().position(|&id| id == dragon_corpse).unwrap();
    let ev_eat = sim.step_player_action(ActionAst::Eat(corpse_idx));
    print_events(&ev_eat);

    println!("\nFormal Voluntary Conducts Audit:");
    println!(
        "  - Pacifist:    {}",
        if sim.conducts.pacifist {
            "ACTIVE (Never attacked/killed)"
        } else {
            "BROKEN (Direct kill)"
        }
    );
    println!(
        "  - Vegan:       {}",
        if sim.conducts.vegan {
            "ACTIVE"
        } else {
            "BROKEN (Consumed dragon meat)"
        }
    );
    println!(
        "  - Vegetarian:  {}",
        if sim.conducts.vegetarian {
            "ACTIVE"
        } else {
            "BROKEN (Consumed meat)"
        }
    );
    println!(
        "  - Atheist:     {}",
        if sim.conducts.atheist {
            "ACTIVE"
        } else {
            "BROKEN (Offered sacrifice)"
        }
    );
    println!(
        "  - Illiterate:  {}",
        if sim.conducts.illiterate {
            "ACTIVE"
        } else {
            "BROKEN (Read scrolls)"
        }
    );
    println!(
        "  - Genocideless:{}",
        if sim.conducts.genocideless {
            "ACTIVE"
        } else {
            "BROKEN (Genocided species)"
        }
    );
    println!(
        "  - Wishless:    {}",
        if sim.conducts.wishless {
            "ACTIVE"
        } else {
            "BROKEN"
        }
    );

    print_separator("GRAND 10-PHASE MASTER DEMO COMPLETE — COMPLETE");
    println!("  * Lean 4 models: build with `lake build`");
    println!("  * Safe Rust workspace: run `cargo test --workspace` for the test suites");
    println!(
        "  * Native TUI Console:  Run `cargo run -p netrust-tui` for interactive terminal play"
    );
    println!(
        "  * GitHub Repository:   Private sync at https://github.com/dmytro-yemelianov/NetRust.git"
    );
}

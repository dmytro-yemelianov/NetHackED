//! Property-Based Tests validating the Lean 4 Theorems in Safe Rust.
//!
//! Every property test here corresponds to a machine-checked theorem in `NetMechanics`.

use netrust_core::{
    calculate_damage, calculate_encumbrance, can_insert_safe, dip_water, identify_fully, learn_buc,
    learn_type, reflect, step_ray, uncurse, BeamRay, Buc, Combatant, DoorState, EncumbranceTier,
    Engraving, EngravingMedium, FormStats, Item, KnowledgeLevel, MetricState, PolyEntity,
    SchedulerState, StepAction, StepResult, SurfaceOrientation, Tile, Velocity, WaterType,
    NORMAL_SPEED, DungeonDepth, hunger_tier, hunger_of_nutrition, SpellKind, cast_spell, mana_cost,
    push_boulder, PushOutcome, branch_entrance_depth, branch_max_depth, enter_branch, exit_branch,
    BranchCoord, BranchId, Coord, Direction,
    feed_pet, interact_with_occupant, swap_displacement, HeroInteraction,
    apply_erosion, enchant_item, mix_alchemy, SAFE_ENCHANT_CAP,
};
use proptest::prelude::*;

prop_compose! {
    fn arb_buc()(idx in 0..3) -> Buc {
        match idx {
            0 => Buc::Blessed,
            1 => Buc::Uncursed,
            _ => Buc::Cursed,
        }
    }
}

prop_compose! {
    fn arb_water()(idx in 0..3) -> WaterType {
        match idx {
            0 => WaterType::Holy,
            1 => WaterType::Plain,
            _ => WaterType::Unholy,
        }
    }
}

prop_compose! {
    fn arb_combatant()(
        hp in 1u32..500,
        max_hp in 1u32..500,
        ac in -30i32..20,
        level in 1u32..30,
        to_hit in -10i32..20,
        dmg_bonus in -5i32..20
    ) -> Combatant {
        let max = hp.max(max_hp);
        Combatant {
            hp,
            max_hp: max,
            ac,
            level,
            to_hit_bonus: to_hit,
            damage_bonus: dmg_bonus,
            is_dead: false,
        }
    }
}

proptest! {
    // -------------------------------------------------------------
    // Theorem: dip_holy_idempotent & dip_unholy_idempotent
    // -------------------------------------------------------------
    #[test]
    fn prop_dip_holy_idempotent(buc in arb_buc()) {
        let once = dip_water(WaterType::Holy, buc);
        let twice = dip_water(WaterType::Holy, once);
        prop_assert_eq!(once, twice);
        prop_assert_eq!(once, Buc::Blessed);
    }

    #[test]
    fn prop_dip_unholy_idempotent(buc in arb_buc()) {
        let once = dip_water(WaterType::Unholy, buc);
        let twice = dip_water(WaterType::Unholy, once);
        prop_assert_eq!(once, twice);
        prop_assert_eq!(once, Buc::Cursed);
    }

    #[test]
    fn prop_dip_plain_always_uncursed(buc in arb_buc()) {
        prop_assert_eq!(dip_water(WaterType::Plain, buc), Buc::Uncursed);
    }

    // -------------------------------------------------------------
    // Theorem: uncurse_idempotent & uncurse_never_cursed
    // -------------------------------------------------------------
    #[test]
    fn prop_uncurse_idempotent(buc in arb_buc()) {
        let once = uncurse(buc);
        let twice = uncurse(once);
        prop_assert_eq!(once, twice);
        prop_assert_ne!(once, Buc::Cursed);
    }

    // -------------------------------------------------------------
    // Theorem: unencumbered_when_le_cap & encumbrance monotonicity
    // -------------------------------------------------------------
    #[test]
    fn prop_unencumbered_when_le_cap(cap in 1u32..5000, ratio in 0.0f64..=1.0) {
        let wt = (cap as f64 * ratio) as u32;
        prop_assert_eq!(calculate_encumbrance(wt, cap), EncumbranceTier::Unencumbered);
    }

    #[test]
    fn prop_encumbrance_monotonic(cap in 1u32..5000, w1 in 0u32..20000, w2 in 0u32..20000) {
        let (lo, hi) = if w1 <= w2 { (w1, w2) } else { (w2, w1) };
        let tier_lo = calculate_encumbrance(lo, cap);
        let tier_hi = calculate_encumbrance(hi, cap);
        prop_assert!(tier_lo <= tier_hi);
    }

    // -------------------------------------------------------------
    // Theorem: scheduler_progress (Progress Theorem)
    // -------------------------------------------------------------
    #[test]
    fn prop_scheduler_progress(
        hero_e in 0u32..50,
        hero_spd in 1u32..30,
        mon_e in 0u32..50,
        mon_spd in 1u32..30
    ) {
        let mut sched = SchedulerState {
            turn: 1,
            hero_energy: hero_e,
            hero_speed: hero_spd,
            monster_energy: mon_e,
            monster_speed: mon_spd,
        };

        let before = sched;
        let action = sched.step();

        match action {
            StepAction::HeroStep => {
                prop_assert_eq!(sched.hero_energy, before.hero_energy - NORMAL_SPEED);
                prop_assert_eq!(sched.turn, before.turn);
            }
            StepAction::MonsterStep => {
                prop_assert_eq!(sched.monster_energy, before.monster_energy - NORMAL_SPEED);
                prop_assert_eq!(sched.turn, before.turn);
            }
            StepAction::TurnTick => {
                prop_assert_eq!(sched.turn, before.turn + 1);
                prop_assert_eq!(sched.hero_energy, before.hero_energy + before.hero_speed);
                prop_assert_eq!(sched.monster_energy, before.monster_energy + before.monster_speed);
            }
        }
    }

    // -------------------------------------------------------------
    // Theorem: apply_damage_monotone_hp & lethal_damage_kills
    // -------------------------------------------------------------
    #[test]
    fn prop_damage_monotone_hp(mut c in arb_combatant(), dmg in 0u32..1000) {
        let hp_before = c.hp;
        c.apply_damage(dmg);
        prop_assert!(c.hp <= hp_before);
        if dmg >= hp_before {
            prop_assert_eq!(c.hp, 0);
            prop_assert!(c.is_dead);
        } else {
            prop_assert_eq!(c.hp, hp_before - dmg);
            prop_assert!(!c.is_dead);
        }
    }

    // -------------------------------------------------------------
    // Theorem: negative_ac_absorbs_damage
    // -------------------------------------------------------------
    #[test]
    fn prop_negative_ac_absorbs_damage(
        roll in 1u32..20,
        enchant in 0i32..10,
        bonus in 0i32..10,
        neg_ac in -20i32..-1
    ) {
        let dmg_no_ac = calculate_damage(roll, enchant, bonus, 0);
        let dmg_neg_ac = calculate_damage(roll, enchant, bonus, neg_ac);
        prop_assert!(dmg_neg_ac <= dmg_no_ac);
        if dmg_no_ac > 0 {
            prop_assert!(dmg_neg_ac >= 1);
        }
    }

    // -------------------------------------------------------------
    // Theorem: break_door_idempotent
    // -------------------------------------------------------------
    #[test]
    fn prop_break_door_idempotent(door_state_idx in 0..4, trapped in any::<bool>()) {
        let state = match door_state_idx {
            0 => DoorState::Open,
            1 => DoorState::Closed,
            2 => DoorState::Locked,
            _ => DoorState::Broken,
        };
        let mut door = Tile::Door { state, trapped };
        door.break_door();
        let once = door.clone();
        door.break_door();
        prop_assert_eq!(&once, &door);
        prop_assert_eq!(&door, &Tile::Door { state: DoorState::Broken, trapped: false });
    }

    // -------------------------------------------------------------
    // Theorem: boh_cannot_contain_boh (Bag of Holding safety)
    // -------------------------------------------------------------
    #[test]
    fn prop_boh_cannot_contain_boh(buc1 in arb_buc(), buc2 in arb_buc()) {
        let boh1 = Item::Box {
            name: "bag 1".into(),
            base_weight: 15,
            buc: buc1,
            is_bag_of_holding: true,
            contents: vec![],
        };
        let boh2 = Item::Box {
            name: "bag 2".into(),
            base_weight: 15,
            buc: buc2,
            is_bag_of_holding: true,
            contents: vec![],
        };
        prop_assert!(!can_insert_safe(&boh2, &boh1));
    }

    // -------------------------------------------------------------
    // Theorem: reflect_involution
    // -------------------------------------------------------------
    #[test]
    fn prop_reflect_involution(
        dx in -100i32..100,
        dy in -100i32..100,
        s_idx in 0u8..3
    ) {
        let s = match s_idx {
            0 => SurfaceOrientation::Horizontal,
            1 => SurfaceOrientation::Vertical,
            _ => SurfaceOrientation::Corner,
        };
        let v = Velocity::new(dx, dy);
        prop_assert_eq!(reflect(reflect(v, s), s), v);
    }

    // -------------------------------------------------------------
    // Theorem: reflect_preserves_speed_sq
    // -------------------------------------------------------------
    #[test]
    fn prop_reflect_speed_sq_conserved(
        dx in -100i32..100,
        dy in -100i32..100,
        s_idx in 0u8..3
    ) {
        let s = match s_idx {
            0 => SurfaceOrientation::Horizontal,
            1 => SurfaceOrientation::Vertical,
            _ => SurfaceOrientation::Corner,
        };
        let v = Velocity::new(dx, dy);
        prop_assert_eq!(reflect(v, s).speed_sq(), v.speed_sq());
    }

    // -------------------------------------------------------------
    // Theorem: step_decreases_energy
    // -------------------------------------------------------------
    #[test]
    fn prop_step_decreases_energy(
        energy in 1u32..100,
        hit_wall in proptest::bool::ANY,
        s_idx in 0u8..3
    ) {
        let wall = if hit_wall {
            Some(match s_idx {
                0 => SurfaceOrientation::Horizontal,
                1 => SurfaceOrientation::Vertical,
                _ => SurfaceOrientation::Corner,
            })
        } else {
            None
        };

        let ray = BeamRay {
            x: 10,
            y: 10,
            vel: Velocity::new(1, -1),
            energy,
        };

        let res = step_ray(ray, wall);
        match res {
            StepResult::Advanced(r_next) | StepResult::Reflected(r_next) => {
                prop_assert_eq!(r_next.energy, energy - 1);
            }
            StepResult::Terminated => {
                prop_assert!(false, "Non-zero energy should not terminate on first step");
            }
        }
    }

    // -------------------------------------------------------------
    // Theorem: burned_engraving_permanent
    // -------------------------------------------------------------
    #[test]
    fn prop_burned_engraving_permanent(text in "[a-zA-Z0-9 ]{1,20}") {
        let e = Engraving::new(&text, EngravingMedium::Burned);
        prop_assert_eq!(e.smudge(), Some(e.clone()));
    }

    // -------------------------------------------------------------
    // Theorem: knowledge_monotone
    // -------------------------------------------------------------
    #[test]
    fn prop_identification_monotonicity(k_idx in 0u8..4) {
        let k = match k_idx {
            0 => KnowledgeLevel::Unidentified,
            1 => KnowledgeLevel::TypeIdentified,
            2 => KnowledgeLevel::BucKnown,
            _ => KnowledgeLevel::FullyIdentified,
        };
        prop_assert!(k <= learn_type(k));
        prop_assert!(k <= learn_buc(k));
        prop_assert!(k <= identify_fully(k));
    }

    // -------------------------------------------------------------
    // Theorem: poly_damage_preserves_base_max_hp
    // -------------------------------------------------------------
    #[test]
    fn prop_poly_damage_preserves_base_max_hp(
        base_hp in 1u32..50,
        poly_hp in 1u32..50,
        damage in 0u32..100
    ) {
        let base = FormStats {
            hp: base_hp,
            max_hp: base_hp,
            name: "Hero".into(),
        };
        let poly = FormStats {
            hp: poly_hp,
            max_hp: poly_hp,
            name: "Wolf".into(),
        };
        let entity = PolyEntity {
            base_form: base,
            poly_form: Some(poly),
        };
        let (after, _) = entity.apply_damage(damage);
        prop_assert_eq!(after.base_form.max_hp, base_hp);
    }

    // -------------------------------------------------------------
    // Theorem: poly_fatal_damage_reverts
    // -------------------------------------------------------------
    #[test]
    fn prop_poly_fatal_damage_reverts(
        poly_hp in 1u32..50,
        extra in 0u32..50
    ) {
        let base = FormStats {
            hp: 20,
            max_hp: 20,
            name: "Hero".into(),
        };
        let poly = FormStats {
            hp: poly_hp,
            max_hp: poly_hp,
            name: "Wolf".into(),
        };
        let entity = PolyEntity {
            base_form: base,
            poly_form: Some(poly),
        };
        let (after, _) = entity.apply_damage(poly_hp + extra);
        prop_assert!(!after.is_polymorphed());
    }

    // -------------------------------------------------------------
    // Theorem: descent_step_decreases_distance
    // -------------------------------------------------------------
    #[test]
    fn prop_descent_decreases_distance(
        curr in 1u32..50,
        next in 0u32..50
    ) {
        let current = MetricState::new(curr);
        let neighbor = Some(MetricState::new(next));
        let step = current.descent_step(neighbor);
        if next < curr {
            prop_assert!(step.dist_to_target < curr);
            prop_assert_eq!(step.dist_to_target, next);
        } else {
            prop_assert_eq!(step.dist_to_target, curr);
        }
    }

    // -------------------------------------------------------------
    // Theorem: ascend_descend_inverse & descend_strictly_increases
    // -------------------------------------------------------------
    #[test]
    fn prop_dungeon_depth_invertibility(depth in 1usize..100) {
        let d = DungeonDepth::new(depth).unwrap();
        let descended = d.descend();
        prop_assert!(descended.as_usize() > d.as_usize());
        prop_assert_eq!(descended.ascend(), d);
    }

    // -------------------------------------------------------------
    // Theorem: eating_improves_or_preserves_hunger
    // -------------------------------------------------------------
    #[test]
    fn prop_eating_improves_hunger(n in 0u32..2000, k in 0u32..2000) {
        let t1 = hunger_tier(hunger_of_nutrition(n));
        let t2 = hunger_tier(hunger_of_nutrition(n + k));
        prop_assert!(t1 <= t2);
    }

    // -------------------------------------------------------------
    // Theorem: cast_preserves_non_negative_mana
    // -------------------------------------------------------------
    #[test]
    fn prop_cast_preserves_mana(pw in 0u32..100, spell_idx in 0usize..4) {
        let spell = match spell_idx {
            0 => SpellKind::ForceBolt,
            1 => SpellKind::MagicMissile,
            2 => SpellKind::CureLightWounds,
            _ => SpellKind::ExtraHealing,
        };
        if let Some(remaining) = cast_spell(pw, spell) {
            prop_assert!(remaining <= pw);
            prop_assert_eq!(remaining, pw - mana_cost(spell));
        } else {
            prop_assert!(pw < mana_cost(spell));
        }
    }

    // -------------------------------------------------------------
    // Theorem: wall_strictly_blocks & pit_push_fills & floor_push_advances
    // -------------------------------------------------------------
    #[test]
    fn prop_boulder_push_theorems(x in 10usize..70, y in 5usize..15, dir_idx in 0usize..4) {
        let dir = match dir_idx {
            0 => Direction::North,
            1 => Direction::East,
            2 => Direction::South,
            _ => Direction::West,
        };
        let pos = Coord::new_unchecked(x, y);
        let next_pos = pos.step(dir).unwrap();

        // Wall strictly blocks
        let wall = Tile::Wall { horizontal: true };
        prop_assert_eq!(push_boulder(pos, dir, &wall, false), PushOutcome::Blocked);

        // Pit push fills
        let pit = Tile::Pit { filled: false };
        prop_assert_eq!(push_boulder(pos, dir, &pit, false), PushOutcome::FilledPit(next_pos));

        // Floor push advances
        let room = Tile::Room;
        prop_assert_eq!(push_boulder(pos, dir, &room, false), PushOutcome::Moved(next_pos));

        // Occupied target strictly blocks
        prop_assert_eq!(push_boulder(pos, dir, &room, true), PushOutcome::Blocked);
    }

    // -------------------------------------------------------------
    // Theorem: corner_deadlock_blocked
    // -------------------------------------------------------------
    #[test]
    fn prop_corner_deadlock(x in 10usize..70, y in 5usize..15) {
        let pos = Coord::new_unchecked(x, y);
        let wall = Tile::Wall { horizontal: true };

        // Pushing North or East into corner walls
        let res_n = push_boulder(pos, Direction::North, &wall, false);
        let res_e = push_boulder(pos, Direction::East, &wall, false);
        prop_assert_eq!(res_n, PushOutcome::Blocked);
        prop_assert_eq!(res_e, PushOutcome::Blocked);
    }

    // -------------------------------------------------------------
    // Theorem: branch_transition_invertible & main_dungeon_no_side_exit
    // -------------------------------------------------------------
    #[test]
    fn prop_branch_theorems(branch_idx in 0usize..3) {
        let branch = match branch_idx {
            0 => BranchId::DungeonsOfDoom,
            1 => BranchId::GnomishMines,
            _ => BranchId::Sokoban,
        };

        prop_assert!(branch_max_depth(branch) >= 3);
        let entrance = branch_entrance_depth(branch);
        let entered = enter_branch(branch, entrance);
        prop_assert_eq!(entered, Some(BranchCoord { branch, depth: 1 }));

        let exited = exit_branch(entered.unwrap());
        prop_assert_eq!(exited, Some(BranchCoord { branch: BranchId::DungeonsOfDoom, depth: entrance }));
    }

    #[test]
    fn prop_main_dungeon_no_side_exit_thm(_dummy in 0..1) {
        prop_assert_eq!(
            exit_branch(BranchCoord { branch: BranchId::DungeonsOfDoom, depth: 1 }),
            Some(BranchCoord { branch: BranchId::DungeonsOfDoom, depth: 1 })
        );
    }

    // -------------------------------------------------------------
    // Theorems: swap_involution & swap_preserves_distance
    // -------------------------------------------------------------
    #[test]
    fn prop_pet_displacement_geometry(hx in 0usize..79, hy in 0usize..20, px in 0usize..79, py in 0usize..20) {
        let hero = Coord::new_unchecked(hx, hy);
        let pet = Coord::new_unchecked(px, py);

        let (h1, p1) = swap_displacement(hero, pet);
        let (h2, p2) = swap_displacement(h1, p1);

        // Involution: swapping twice restores original positions
        prop_assert_eq!((h2, p2), (hero, pet));

        // Distance conservation
        prop_assert_eq!(hero.chebyshev_distance(pet), h1.chebyshev_distance(p1));
    }

    // -------------------------------------------------------------
    // Theorems: feed_increases_tameness & feed_preserves_tame
    // -------------------------------------------------------------
    #[test]
    fn prop_feed_pet_tameness(tameness in 1u32..100, nutrition in 0u32..1000) {
        let (new_tameness, is_tame) = feed_pet(tameness, true, nutrition);
        prop_assert!(new_tameness > tameness);
        prop_assert!(is_tame);
    }

    // -------------------------------------------------------------
    // Theorem: displacement_non_violent
    // -------------------------------------------------------------
    #[test]
    fn prop_pet_displacement_non_violent(hx in 0usize..79, hy in 0usize..20, px in 0usize..79, py in 0usize..20) {
        let hero = Coord::new_unchecked(hx, hy);
        let pet = Coord::new_unchecked(px, py);
        let dummy_id = 42usize;

        let interaction_tame = interact_with_occupant(hero, pet, dummy_id, true);
        prop_assert_eq!(
            interaction_tame,
            HeroInteraction::DisplacePet {
                pet_id: dummy_id,
                new_hero_pos: pet,
                new_pet_pos: hero,
            }
        );

        let interaction_hostile = interact_with_occupant(hero, pet, dummy_id, false);
        prop_assert_eq!(interaction_hostile, HeroInteraction::MeleeAttack(dummy_id));
    }

    // -------------------------------------------------------------
    // Theorems: enchant_below_cap_safe & enchant_below_cap_increases
    // -------------------------------------------------------------
    #[test]
    fn prop_enchant_theorems(cur_ench in -5i8..SAFE_ENCHANT_CAP, blessed in proptest::bool::ANY) {
        let res = enchant_item(cur_ench, blessed, false);
        prop_assert!(!res.evaporated);
        prop_assert!(res.new_ench > cur_ench);
    }

    #[test]
    fn prop_enchant_at_or_above_cap(cur_ench in SAFE_ENCHANT_CAP..20i8, blessed in proptest::bool::ANY) {
        let res = enchant_item(cur_ench, blessed, false);
        prop_assert!(res.evaporated);
        prop_assert_eq!(res.new_ench, cur_ench);
    }

    // -------------------------------------------------------------
    // Theorems: proofed_impermeable & erosion_monotonic
    // -------------------------------------------------------------
    #[test]
    fn prop_erosion_theorems(cur_erosion in 0u8..=4, proofed in proptest::bool::ANY) {
        let next_erosion = apply_erosion(cur_erosion, proofed);
        if proofed {
            prop_assert_eq!(next_erosion, cur_erosion);
        } else {
            prop_assert!(next_erosion >= cur_erosion);
        }
    }

    // -------------------------------------------------------------
    // Theorem: alchemy_healing_energy_commutative
    // -------------------------------------------------------------
    #[test]
    fn prop_alchemy_theorems(_dummy in 0..1) {
        let mix1 = mix_alchemy("potion of healing", "potion of speed");
        let mix2 = mix_alchemy("potion of speed", "potion of healing");
        prop_assert_eq!(mix1, mix2);
        prop_assert_eq!(mix1, Some("potion of extra healing"));
    }
}




//! Property-Based Tests validating the Lean 4 Theorems in Safe Rust.
//!
//! Every property test here corresponds to a machine-checked theorem in `NetMechanics`.

use netrust_core::{
    apply_erosion, apply_priest_donation, apply_vorpal_strike, attack_hits, attack_nemesis,
    branch_entrance_depth, branch_max_depth, buy_factor, calculate_buy_price, calculate_damage,
    calculate_encumbrance, calculate_mysterious_force, calculate_sell_price,
    calculate_summon_count, calculate_tournament_score, can_detect_monster, can_insert_safe,
    can_see_tile, cast_spell, choose_pet_goal, clamp_favor, consecrate_water, consult_leader,
    corrupt_buc_on_death, create_ghost_hp, decide_tactical_action, destroy_drawbridge,
    dilute_potion, dip_water, enchant_item, enter_branch, exit_branch, feed_pet,
    hero_damage_after_ac, hunger_of_nutrition, hunger_tier, identify_fully, interact_with_occupant,
    is_candelabrum_ready, is_hero_eligible_for_quest, is_hp_critical, is_sanctum_accessible,
    is_valid_bones_level, learn_buc, learn_type, luck_decay_period, mana_cost, melee_damage,
    mix_alchemy, monster_to_hit_value, offer_amulet_on_high_altar, pet_tile_steppable,
    pick_up_quest_artifact, priest_uncurse, promote_pet, protection_donation_cost, push_boulder,
    quest_progress_rank, recharge_wand, reflect, resolve_breath_damage, resolve_gaze,
    resolve_sacrifice, return_to_leader_with_artifact, rub_lamp, sell_factor, step_luck_decay,
    step_ray, step_ritual, swap_displacement, tick_light_fuel, tick_prayer_timeout, to_hit_value,
    toggle_drawbridge, uncurse, zap_wand, Alignment, ArtifactLocation, AscensionOutcome, BeamRay,
    BranchCoord, BranchId, BreathType, Buc, CandelabrumState, Combatant, Coord, DilutionState,
    Direction, DivineState, DoorState, DrawbridgeState, DrawbridgeTransition, DungeonDepth,
    EncumbranceTier, Engraving, EngravingMedium, FormStats, GazeEffect, GazeType, HeroInteraction,
    HeroQuestEligibility, Intrinsics, InvocationStep, Item, KnowledgeLevel, LightSource,
    MetricState, PetFamily, PetGoal, PetSpeciesTier, PolyEntity, PushOutcome, QuestProgress,
    QuestState, RechargeResult, RitualProgress, RubResult, SchedulerState, SpellKind, StepAction,
    StepResult, SurfaceOrientation, TacticalAction, TacticalContext, Tile, Velocity, WandCharges,
    WaterType, MAX_DIVINE_PROTECTION, NORMAL_SPEED, QUEST_MIN_ALIGNMENT, QUEST_MIN_LEVEL,
    REQUIRED_CANDLES, SAFE_ENCHANT_CAP,
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

    /// Reference from `hack.c:4372` (`calc_capacity`) and `hack.c:4295` (`weight_cap`).
    #[test]
    fn prop_encumbrance_matches_c_reference(wt in 0u32..20000, cap in 1u32..5000) {
        let c_tier: i64 = {
            let (w, wc) = (wt as i64, cap as i64);
            let diff = w - wc;
            if diff <= 0 { 0 } else if wc <= 1 { 5 } else { (diff * 2 / wc + 1).min(5) }
        };
        let want = match c_tier {
            0 => EncumbranceTier::Unencumbered,
            1 => EncumbranceTier::Burdened,
            2 => EncumbranceTier::Stressed,
            3 => EncumbranceTier::Strained,
            4 => EncumbranceTier::Overtaxed,
            _ => EncumbranceTier::Overloaded,
        };
        prop_assert_eq!(netrust_core::encumbrance_tier(wt, cap), want);
    }

    #[test]
    fn prop_weight_cap_matches_c_reference(
        st in -5i32..40, con in -5i32..40, lev in any::<bool>(), legs in 0u8..4
    ) {
        let mut carrcap: i64 = 25 * (st as i64 + con as i64) + 50;
        if lev {
            carrcap = 1000;
        } else {
            if carrcap > 1000 { carrcap = 1000; }
            carrcap -= 100 * (legs.min(2) as i64);
        }
        if carrcap < 1 { carrcap = 1; }
        let got = netrust_core::weight_cap(st, con, lev, legs);
        prop_assert_eq!(got as i64, carrcap);
        prop_assert!((1..=1000).contains(&got));
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
    // C reference: uhitm.c:365 find_roll_to_hit, uhitm.c:780 hit test
    // -------------------------------------------------------------
    #[test]
    fn prop_to_hit_matches_c_reference(
        level in -5i32..40,
        luck in -30i32..30,
        enchant in -10i32..10,
        skill_hit in -4i32..8,
        target_ac in -40i32..20,
        d20 in 0u32..30
    ) {
        // Reference written directly from C:
        // tmp = 1 + abon(0) + find_mac + ulevel + sgn(Luck)*((|Luck|+2)/3) + spe + skill
        let l = luck.clamp(-13, 13);
        let luck_term = l.signum() * ((l.abs() + 2) / 3);
        let tmp = 1 + target_ac + level + luck_term + enchant + skill_hit;
        prop_assert_eq!(to_hit_value(level, luck, enchant, skill_hit, target_ac), tmp);
        // dieroll = rnd(20); mhit = tmp > dieroll
        let dieroll = d20.clamp(1, 20) as i32;
        prop_assert_eq!(attack_hits(d20, tmp), tmp > dieroll);
    }

    // -------------------------------------------------------------
    // C reference: mhitu.c:709 monster -> hero to-hit, hack.h:1538 AC_VALUE
    // -------------------------------------------------------------
    #[test]
    fn prop_monster_to_hit_matches_c_reference(
        m_lev in 0i32..50,
        hero_ac in -40i32..20,
        ac_roll in 0u32..50
    ) {
        let ac_value = if hero_ac >= 0 {
            hero_ac
        } else {
            -(ac_roll.clamp(1, (-hero_ac) as u32) as i32)
        };
        let tmp = (ac_value + 10 + m_lev).max(1);
        prop_assert_eq!(monster_to_hit_value(m_lev, hero_ac, ac_roll), tmp);
    }

    // -------------------------------------------------------------
    // C reference: uhitm.c:1505 (min 1) and mhitu.c:1208 (rnd(-uac) absorb)
    // -------------------------------------------------------------
    #[test]
    fn prop_hero_ac_absorb_matches_c(
        roll in 0u32..30,
        enchant in -10i32..10,
        bonus in -5i32..10,
        hero_ac in -40i32..20,
        absorb_roll in 0u32..50
    ) {
        // dmg = base + spe + bonus; if (dmg < 1) dmg = 1;
        let base = (roll as i32 + enchant + bonus).max(1) as u32;
        prop_assert_eq!(melee_damage(roll, enchant, bonus), base);
        // if (dmg && u.uac < 0) { dmg -= rnd(-u.uac); if (dmg < 1) dmg = 1; }
        let expected = if base > 0 && hero_ac < 0 {
            let r = absorb_roll.clamp(1, (-hero_ac) as u32) as i32;
            (base as i32 - r).max(1) as u32
        } else {
            base
        };
        prop_assert_eq!(hero_damage_after_ac(base, hero_ac, absorb_roll), expected);
        prop_assert_eq!(
            calculate_damage(roll, enchant, bonus, hero_ac, Some(absorb_roll)),
            expected
        );
        // Hero -> monster: no AC reduction at all.
        prop_assert_eq!(calculate_damage(roll, enchant, bonus, hero_ac, None), base);
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
    fn prop_eating_improves_hunger(n in -3000i32..3000, k in 0i32..3000, con in 3i32..26) {
        let t1 = hunger_tier(hunger_of_nutrition(n, con));
        let t2 = hunger_tier(hunger_of_nutrition(n + k, con));
        prop_assert!(t1 <= t2);
    }

    /// Reference from `eat.c:3362` (`newuhs`) and `eat.c:3437` (starvation).
    #[test]
    fn prop_hunger_matches_c_reference(h in -3000i32..3000, con in 3i32..26) {
        let want = if h > 1000 { netrust_core::HungerState::Satiated }
            else if h > 150 { netrust_core::HungerState::Normal }
            else if h > 50 { netrust_core::HungerState::Hungry }
            else if h > 0 { netrust_core::HungerState::Weak }
            else if h < -(100 + 10 * con) { netrust_core::HungerState::Starved }
            else { netrust_core::HungerState::Fainting };
        prop_assert_eq!(hunger_of_nutrition(h, con), want);
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

    // -------------------------------------------------------------
    // Theorems: open_drawbridge_is_passable & raise_crushes_occupant_fatal
    // -------------------------------------------------------------
    #[test]
    fn prop_drawbridge_theorems(has_occupant in proptest::bool::ANY) {
        // Lowering a closed drawbridge
        let (opened_state, trans_lowered) = toggle_drawbridge(DrawbridgeState::Closed, has_occupant);
        prop_assert_eq!(opened_state, DrawbridgeState::Open);
        prop_assert_eq!(trans_lowered, DrawbridgeTransition::Lowered);

        // Raising an open drawbridge
        let (closed_state, trans_raised) = toggle_drawbridge(DrawbridgeState::Open, has_occupant);
        prop_assert_eq!(closed_state, DrawbridgeState::Closed);
        if has_occupant {
            prop_assert_eq!(trans_raised, DrawbridgeTransition::Raised { crushed_damage: 9999 });
        } else {
            prop_assert_eq!(trans_raised, DrawbridgeTransition::Raised { crushed_damage: 0 });
        }

        // Destroying drawbridge
        let (destroyed_state, trans_destroyed) = destroy_drawbridge();
        prop_assert_eq!(destroyed_state, DrawbridgeState::Destroyed);
        prop_assert_eq!(trans_destroyed, DrawbridgeTransition::DestroyedAndFellInMoat);
    }

    // -------------------------------------------------------------
    // Theorems: ascension_requires_real_amulet & ascension_iff_aligned
    // -------------------------------------------------------------
    #[test]
    fn prop_ascension_theorems(
        has_real_amulet in proptest::bool::ANY,
        h_idx in 0usize..3,
        a_idx in 0usize..3,
    ) {
        let aligns = [Alignment::Lawful, Alignment::Neutral, Alignment::Chaotic];
        let hero_align = aligns[h_idx];
        let altar_align = aligns[a_idx];

        let outcome = offer_amulet_on_high_altar(has_real_amulet, hero_align, altar_align);

        if !has_real_amulet {
            prop_assert!(matches!(outcome, AscensionOutcome::Rejected(_)));
        } else if hero_align == altar_align {
            prop_assert_eq!(outcome, AscensionOutcome::Ascended(altar_align));
        } else {
            prop_assert!(matches!(outcome, AscensionOutcome::Rejected(_)));
        }
    }

    // -------------------------------------------------------------
    // Theorems: clamp_favor_bounded & prayer_cooldown_strictly_decreases
    // -------------------------------------------------------------
    #[test]
    fn prop_clamp_favor_bounded(f in -100i32..100) {
        let clamped = clamp_favor(f);
        prop_assert!((-20..=20).contains(&clamped));
    }

    #[test]
    fn prop_prayer_cooldown_strictly_decreases(timeout in 1u32..500) {
        let next_timeout = tick_prayer_timeout(timeout);
        prop_assert!(next_timeout < timeout);
    }

    // -------------------------------------------------------------
    // Theorem: sacrifice_coaligned_increases_favor
    // -------------------------------------------------------------
    #[test]
    fn prop_sacrifice_favor_monotonic(
        init_favor in -20i32..=17,
        nutr in 0u32..1000,
        a_idx in 0usize..3,
    ) {
        let aligns = [Alignment::Lawful, Alignment::Neutral, Alignment::Chaotic];
        let align = aligns[a_idx];
        let state = DivineState {
            favor: init_favor,
            prayer_timeout: 0,
            gift_count: 0,
        };
        let (new_state, _) = resolve_sacrifice(state, align, align, nutr);
        prop_assert!(new_state.favor > init_favor);
    }

    // -------------------------------------------------------------
    // Theorem: consecrate_water_yields_blessed
    // -------------------------------------------------------------
    #[test]
    fn prop_consecrate_water_theorems(
        favor in -20i32..=20,
        is_coaligned in proptest::bool::ANY,
    ) {
        let res = consecrate_water(Buc::Uncursed, is_coaligned, favor);
        if is_coaligned && favor > 5 {
            prop_assert_eq!(res, Buc::Blessed);
        } else {
            prop_assert_eq!(res, Buc::Uncursed);
        }
    }

    // -------------------------------------------------------------
    // Theorem: vorpal_decapitation_fatal
    // -------------------------------------------------------------
    #[test]
    fn prop_vorpal_decapitation_fatal(hp in 1u32..500) {
        let (new_hp, is_dead) = apply_vorpal_strike(hp, true);
        prop_assert_eq!(new_hp, 0);
        prop_assert!(is_dead);
    }

    // -------------------------------------------------------------
    // Theorems: wand_charge_depletes & empty_wand_cannot_zap
    // -------------------------------------------------------------
    #[test]
    fn prop_wand_charge_depletes(charges in 1u32..50, recharges in 0u32..3) {
        let w = WandCharges { charges, recharges };
        let zapped = zap_wand(w);
        prop_assert!(zapped.is_some());
        prop_assert_eq!(zapped.unwrap().charges, charges - 1);

        let empty = WandCharges { charges: 0, recharges };
        prop_assert_eq!(zap_wand(empty), None);
    }

    // -------------------------------------------------------------
    // Theorems: recharge_safe_below_cap & recharge_explodes_at_cap
    // -------------------------------------------------------------
    #[test]
    fn prop_recharge_theorems(
        charges in 0u32..10,
        recharges in 0u32..10,
        add in 1u32..5,
    ) {
        let w = WandCharges { charges, recharges };
        let res = recharge_wand(w, add);
        if recharges >= 3 {
            prop_assert_eq!(res, RechargeResult::Exploded);
        } else {
            prop_assert_eq!(res, RechargeResult::Success(WandCharges {
                charges: charges + add,
                recharges: recharges + 1,
            }));
        }
    }

    // -------------------------------------------------------------
    // Theorem: breath_damage_le_raw
    // -------------------------------------------------------------
    #[test]
    fn prop_breath_damage_bounded(
        raw in 0u32..200,
        b_idx in 0usize..6,
        fire_res in proptest::bool::ANY,
        cold_res in proptest::bool::ANY,
        shock_res in proptest::bool::ANY,
        reflect in proptest::bool::ANY,
    ) {
        let breaths = [
            BreathType::Fire,
            BreathType::Cold,
            BreathType::Shock,
            BreathType::Sleep,
            BreathType::Poison,
            BreathType::Disintegration,
        ];
        let breath = breaths[b_idx];
        let mut intrinsics = Intrinsics::empty();
        intrinsics.fire_resistance = fire_res;
        intrinsics.cold_resistance = cold_res;
        intrinsics.shock_resistance = shock_res;
        intrinsics.reflection = reflect;

        let (dmg, was_reflected) = resolve_breath_damage(raw, breath, &intrinsics);
        prop_assert!(dmg <= raw);
        if reflect {
            prop_assert_eq!(dmg, 0);
            prop_assert!(was_reflected);
        }
    }

    // -------------------------------------------------------------
    // Theorems: gaze_reflection_immune & gaze_blindness_immune
    // -------------------------------------------------------------
    #[test]
    fn prop_gaze_invariants(
        g_idx in 0usize..3,
        has_reflection in proptest::bool::ANY,
        is_blind in proptest::bool::ANY,
    ) {
        let gazes = [GazeType::Paralysis, GazeType::Petrification, GazeType::Confusion];
        let gaze = gazes[g_idx];
        let res = resolve_gaze(gaze, has_reflection, is_blind);

        if has_reflection {
            prop_assert_eq!(res, GazeEffect::ReflectedToAttacker);
        } else if is_blind {
            prop_assert_eq!(res, GazeEffect::BlindImmune);
        } else {
            prop_assert_eq!(res, GazeEffect::Afflicted(gaze));
        }
    }

    // -------------------------------------------------------------
    // Theorem: summon_count_bounded
    // -------------------------------------------------------------
    #[test]
    fn prop_summon_count_bounded(
        cur in 0usize..50,
        cap in 0usize..50,
        desired in 0usize..20,
    ) {
        let count = calculate_summon_count(cur, cap, desired);
        prop_assert!(cur + count <= cap.max(cur));
    }

    // -------------------------------------------------------------
    // Bones cursing vs reference from bones.c:290-291 / resetobjs
    // -------------------------------------------------------------
    #[test]
    fn prop_corrupt_buc_matches_c_reference(
        buc in arb_buc(),
        quest in proptest::bool::ANY,
        roll in 0u32..8,
    ) {
        // C: `if (rn2(5)) curse(otmp);` rn2(5) in 0..=4; quest items always cursed.
        fn reference(b: Buc, quest: bool, r: u32) -> Buc {
            if quest || r.min(4) != 0 { Buc::Cursed } else { b }
        }
        let got = corrupt_buc_on_death(buc, quest, roll);
        prop_assert_eq!(got, reference(buc, quest, roll));
        // idempotent for any fixed roll
        prop_assert_eq!(corrupt_buc_on_death(got, quest, roll), got);
    }

    // -------------------------------------------------------------
    // Theorems: ghost_hp_bounded & ghost_hp_strictly_positive
    // -------------------------------------------------------------
    #[test]
    fn prop_ghost_hp_theorems(hp in 0u32..500) {
        let ghost_hp = create_ghost_hp(hp);
        prop_assert!(ghost_hp >= 1);
        if hp >= 1 {
            prop_assert_eq!(ghost_hp, hp);
        }
    }

    // -------------------------------------------------------------
    // Theorem: valid_bones_level_ge_one
    // -------------------------------------------------------------
    #[test]
    fn prop_valid_bones_level_theorems(depth in 0u32..100) {
        let is_valid = is_valid_bones_level(depth);
        prop_assert_eq!(is_valid, depth >= 1);
    }

    // -------------------------------------------------------------
    // Theorems: dilute_water_idempotent & dilute_eventually_water
    // -------------------------------------------------------------
    #[test]
    fn prop_dilution_theorems(tier in 0u32..10) {
        prop_assert_eq!(dilute_potion(DilutionState::Water), DilutionState::Water);

        let mut current = DilutionState::Potion(tier);
        let mut steps = 0;
        while current != DilutionState::Water && steps <= 15 {
            current = dilute_potion(current);
            steps += 1;
        }
        prop_assert_eq!(current, DilutionState::Water);
        prop_assert_eq!(steps, tier + 1);
    }

    // -------------------------------------------------------------
    // Theorems: rub_magic_lamp_exhausts_djinni & rub_oil_lamp_never_wishes
    // -------------------------------------------------------------
    #[test]
    fn prop_rub_lamp_theorems(buc in arb_buc(), turns in 0u32..2000) {
        // Magic lamp with Djinni always consumes Djinni
        let (_, consumed) = rub_lamp(true, true, buc, turns);
        prop_assert!(consumed);

        // Ordinary oil lamp never grants wishes
        let (res, _) = rub_lamp(false, false, buc, turns);
        prop_assert_ne!(res, RubResult::WishGranted);
    }

    // -------------------------------------------------------------
    // Theorems: sell_le_buy_price & charisma factor monotonicity
    // -------------------------------------------------------------
    #[test]
    fn prop_price_identification_theorems(base in 1u32..2000, cha in 1u32..25, buc in arb_buc()) {
        let buy = calculate_buy_price(base, cha, buc);
        let sell = calculate_sell_price(base, cha, buc);
        // Arbitrage prevention: sell price <= buy price
        prop_assert!(sell <= buy);

        // Charisma monotonicity: higher charisma -> buy factor does not increase, sell factor does not decrease
        if cha < 25 {
            let next_buy_fac = buy_factor(cha + 1);
            let cur_buy_fac = buy_factor(cha);
            prop_assert!(next_buy_fac <= cur_buy_fac);

            let next_sell_fac = sell_factor(cha + 1);
            let cur_sell_fac = sell_factor(cha);
            prop_assert!(next_sell_fac >= cur_sell_fac);
        }
    }

    // -------------------------------------------------------------
    // Theorems: pet_rejects_cursed_tile & pet_accepts_safe_tile
    // -------------------------------------------------------------
    #[test]
    fn prop_pet_buc_detection_theorems(b1 in arb_buc(), b2 in arb_buc(), b3 in arb_buc()) {
        let items = vec![b1, b2, b3];
        let has_cursed = items.contains(&Buc::Cursed);
        let steppable = pet_tile_steppable(&items);
        if has_cursed {
            prop_assert!(!steppable);
        } else {
            prop_assert!(steppable);
        }
    }

    // -------------------------------------------------------------
    // Theorems: promote_preserves_family & promote_monotonic_level
    // -------------------------------------------------------------
    #[test]
    fn prop_pet_promotion_theorems(species_idx in 0usize..6, l1 in 1u32..20, l2 in 1u32..20) {
        let species = match species_idx {
            0 => PetSpeciesTier::LittleDog,
            1 => PetSpeciesTier::Dog,
            2 => PetSpeciesTier::WarDog,
            3 => PetSpeciesTier::Kitten,
            4 => PetSpeciesTier::Housecat,
            _ => PetSpeciesTier::LargeCat,
        };

        // Preserves biological family
        let promoted1 = promote_pet(species, l1);
        prop_assert_eq!(promoted1.family(), species.family());
        prop_assert!(matches!(promoted1.family(), PetFamily::Canine | PetFamily::Feline));

        // Monotonic level progression
        if l1 <= l2 {
            let promoted2 = promote_pet(species, l2);
            prop_assert!(promoted1.power_tier() <= promoted2.power_tier());
        }

        // Fixed points at apex tier
        prop_assert_eq!(promote_pet(PetSpeciesTier::WarDog, l1), PetSpeciesTier::WarDog);
        prop_assert_eq!(promote_pet(PetSpeciesTier::LargeCat, l1), PetSpeciesTier::LargeCat);
    }

    // -------------------------------------------------------------
    // Theorem: pet_prioritizes_hero_defense
    // -------------------------------------------------------------
    #[test]
    fn prop_pet_tactical_defense_priority(hostile_id in 0usize..100) {
        let goal = choose_pet_goal(Some(hostile_id), Some(Coord::new_unchecked(10, 10)));
        prop_assert_eq!(goal, PetGoal::AttackHostile(hostile_id));
    }

    // -------------------------------------------------------------
    // Theorem: priest_protection_bounded & priest_protection_monotonic
    // -------------------------------------------------------------
    #[test]
    fn prop_priest_protection_theorems(
        cur in 0u32..=9,
        donation in 0u32..=10000,
        level in 1u32..=30,
    ) {
        let res = apply_priest_donation(cur, donation, level);
        // Bounded by MAX_DIVINE_PROTECTION (9)
        prop_assert!(res <= MAX_DIVINE_PROTECTION);
        // Monotonic
        prop_assert!(cur <= res);

        // Insufficient donation leaves protection unchanged
        if donation < protection_donation_cost(level) && cur < MAX_DIVINE_PROTECTION {
            prop_assert_eq!(res, cur);
        }
    }

    // -------------------------------------------------------------
    // Theorem: priest_uncurse_never_cursed
    // -------------------------------------------------------------
    #[test]
    fn prop_priest_uncurse_theorems(buc in arb_buc()) {
        let purified = priest_uncurse(buc);
        prop_assert_ne!(purified, Buc::Cursed);
    }

    // -------------------------------------------------------------
    // Luck timeout vs reference from timeout.c:595-620 / attrib.c:423
    // -------------------------------------------------------------
    #[test]
    fn prop_luck_timeout_matches_c_reference(
        luck in -10i32..=10,
        base in -2i32..=2,
        stone_idx in 0u32..4,
        amulet in proptest::bool::ANY,
        angry in proptest::bool::ANY,
    ) {
        let stone = match stone_idx {
            0 => None,
            1 => Some(Buc::Blessed),
            2 => Some(Buc::Uncursed),
            _ => Some(Buc::Cursed),
        };
        // C: time_luck = stone_luck(FALSE) (blessed +1, cursed -1, uncursed 0);
        // nostone = !carrying(LUCKSTONE) && !stone_luck(TRUE).
        let time_luck = match stone { Some(Buc::Blessed) => 1, Some(Buc::Cursed) => -1, _ => 0 };
        let nostone = stone.is_none();
        let mut want = luck;
        if luck > base && (nostone || time_luck < 0) {
            want -= 1;
        } else if luck < base && (nostone || time_luck > 0) {
            want += 1;
        }
        prop_assert_eq!(step_luck_decay(luck, base, stone), want);
        prop_assert_eq!(
            luck_decay_period(amulet, angry),
            if amulet || angry { 300 } else { 600 }
        );
    }

    // -------------------------------------------------------------
    // Theorems: blind_blocks_sight & dark_room_requires_light
    // -------------------------------------------------------------
    #[test]
    fn prop_lighting_perception_theorems(
        dist in 0u32..20,
        is_dark in proptest::bool::ANY,
        is_illuminated in proptest::bool::ANY,
    ) {
        // Blindness unconditionally extinguishes direct visual sight of tiles
        prop_assert!(!can_see_tile(true, dist, is_dark, is_illuminated));

        // In dark room, tiles beyond melee radius (1) require illumination
        if is_dark && dist > 1 && !is_illuminated {
            prop_assert!(!can_see_tile(false, dist, is_dark, is_illuminated));
        }

        // In illuminated dark room, tile is visible
        if is_dark && is_illuminated {
            prop_assert!(can_see_tile(false, dist, is_dark, is_illuminated));
        }

        // In standard illuminated room, tile is visible
        if !is_dark {
            prop_assert!(can_see_tile(false, dist, is_dark, is_illuminated));
        }
    }

    // -------------------------------------------------------------
    // Theorems: light_radius_bounded & fuel_monotonically_decreases
    // -------------------------------------------------------------
    #[test]
    fn prop_light_source_bounds_and_fuel_theorems(
        radius in 0u32..10,
        fuel in 0u32..1000,
        is_lit in proptest::bool::ANY,
    ) {
        let ls = LightSource { radius, fuel, is_lit };
        // effectiveRadius <= radius
        prop_assert!(ls.effective_radius() <= ls.radius);

        if !ls.is_active() {
            prop_assert_eq!(ls.effective_radius(), 0);
        } else {
            prop_assert_eq!(ls.effective_radius(), ls.radius);
        }

        // tick_light_fuel <= fuel
        let next_fuel = tick_light_fuel(fuel);
        prop_assert!(next_fuel <= fuel);
        if fuel == 0 {
            prop_assert_eq!(next_fuel, 0);
        } else {
            prop_assert_eq!(next_fuel, fuel - 1);
        }
    }

    // -------------------------------------------------------------
    // Theorems: telepathy_detects_thinking_monsters & mindless_undetected_when_blind
    // -------------------------------------------------------------
    #[test]
    fn prop_telepathy_theorems(
        blind in proptest::bool::ANY,
        telepathy in proptest::bool::ANY,
        has_mind in proptest::bool::ANY,
        tile_visible in proptest::bool::ANY,
    ) {
        let detected = can_detect_monster(blind, telepathy, has_mind, tile_visible);

        // Visual sight guarantees detection
        if tile_visible {
            prop_assert!(detected);
        }

        // Telepathy detects conscious minds regardless of blindness
        if telepathy && has_mind {
            prop_assert!(detected);
        }

        // Mindless monsters undetected when blind and tile not visible
        if blind && !tile_visible && !has_mind {
            prop_assert!(!detected);
        }
    }

    // -------------------------------------------------------------
    // Theorems: critical_hp_elbereth_priority & pet_test_priority
    // -------------------------------------------------------------
    #[test]
    fn prop_tactical_decision_theorems(
        hp in 1u32..100,
        max_hp in 1u32..100,
        has_hostile_adj in proptest::bool::ANY,
        has_pet_adj in proptest::bool::ANY,
        has_unchecked_floor_item in proptest::bool::ANY,
        on_stairs_down in proptest::bool::ANY,
    ) {
        let ctx = TacticalContext {
            hp,
            max_hp: max_hp.max(hp), // hp <= max_hp
            has_hostile_adj,
            has_pet_adj,
            has_unchecked_floor_item,
            on_stairs_down,
        };

        let action = decide_tactical_action(&ctx);

        // Theorem: critical HP with adjacent hostiles strictly triggers Elbereth
        if is_hp_critical(ctx.hp, ctx.max_hp) && ctx.has_hostile_adj {
            prop_assert_eq!(action, TacticalAction::EngraveElbereth);
        }

        // Theorem: unchecked item with pet present triggers pet testing wait
        if !ctx.has_hostile_adj && ctx.has_unchecked_floor_item && ctx.has_pet_adj {
            prop_assert_eq!(action, TacticalAction::WaitPetTest);
        }
    }

    // -------------------------------------------------------------
    // Theorems: tournament_score_depth_monotonic & kills_monotonic
    // -------------------------------------------------------------
    #[test]
    fn prop_tournament_scoring_monotonicity(
        turns in 1u64..1000,
        depth1 in 1u64..10,
        depth2 in 1u64..10,
        kills1 in 0u64..50,
        kills2 in 0u64..50,
        gold in 0u64..1000,
    ) {
        let s_depth1 = calculate_tournament_score(turns, depth1.min(depth2), kills1, gold);
        let s_depth2 = calculate_tournament_score(turns, depth1.max(depth2), kills1, gold);
        prop_assert!(s_depth1 <= s_depth2);

        let s_kills1 = calculate_tournament_score(turns, depth1, kills1.min(kills2), gold);
        let s_kills2 = calculate_tournament_score(turns, depth1, kills1.max(kills2), gold);
        prop_assert!(s_kills1 <= s_kills2);
    }

    // -------------------------------------------------------------
    // Theorems: candelabrum_requires_seven_candles & unlit_candelabrum_not_ready
    // -------------------------------------------------------------
    #[test]
    fn prop_candelabrum_ready_theorems(
        candle_count in 0u32..20,
        is_lit in any::<bool>(),
    ) {
        let cand = CandelabrumState { candle_count, is_lit };
        let ready = is_candelabrum_ready(&cand);

        // Theorem: candelabrum_requires_seven_candles
        if candle_count != REQUIRED_CANDLES {
            prop_assert!(!ready);
        }

        // Theorem: unlit_candelabrum_not_ready
        if !is_lit {
            prop_assert!(!ready);
        }

        if candle_count == REQUIRED_CANDLES && is_lit {
            prop_assert!(ready);
        }
    }

    // -------------------------------------------------------------
    // Theorems: reading_book_off_vibrating_square_fails,
    //           full_ritual_unlocks_sanctum, sanctum_opening_is_permanent
    // -------------------------------------------------------------
    #[test]
    fn prop_invocation_ritual_theorems(
        on_vibrating_square in any::<bool>(),
        is_lit in any::<bool>(),
        candle_count in 0u32..10,
    ) {
        let cand = CandelabrumState { candle_count, is_lit };

        // Theorem: full_ritual_unlocks_sanctum
        if is_candelabrum_ready(&cand) {
            let s1 = step_ritual(RitualProgress::Uninitiated, InvocationStep::RingBell, true, &cand);
            prop_assert_eq!(s1, RitualProgress::BellResounding);

            let s2 = step_ritual(s1, InvocationStep::LightCandelabrum, true, &cand);
            prop_assert_eq!(s2, RitualProgress::CandlesBurning);

            let s3 = step_ritual(s2, InvocationStep::ReadBook, true, &cand);
            prop_assert_eq!(s3, RitualProgress::SanctumOpened);
            prop_assert!(is_sanctum_accessible(s3));
        }

        // Theorem: reading_book_off_vibrating_square_fails
        let s_off = step_ritual(RitualProgress::CandlesBurning, InvocationStep::ReadBook, false, &cand);
        prop_assert_ne!(s_off, RitualProgress::SanctumOpened);
        prop_assert!(!is_sanctum_accessible(s_off));

        // Theorem: sanctum_opening_is_permanent
        for step in [InvocationStep::RingBell, InvocationStep::LightCandelabrum, InvocationStep::ReadBook] {
            let next = step_ritual(RitualProgress::SanctumOpened, step, on_vibrating_square, &cand);
            prop_assert_eq!(next, RitualProgress::SanctumOpened);
            prop_assert!(is_sanctum_accessible(next));
        }
    }

    // -------------------------------------------------------------
    // Theorem: mysterious_force_bounds
    // -------------------------------------------------------------
    #[test]
    fn prop_mysterious_force_bounds(
        depth in 1usize..100,
        roll in any::<u32>(),
    ) {
        let result = calculate_mysterious_force(depth, roll);
        if roll % 3 == 0 {
            prop_assert!(result.is_some());
            let pushed_depth = result.unwrap();
            prop_assert!(pushed_depth > depth);
            prop_assert!(pushed_depth <= depth + 3);
        } else {
            prop_assert!(result.is_none());
        }
    }

    // -------------------------------------------------------------
    // Theorems: Quest Leader qualification, Nemesis combat & Artifact invariants
    // -------------------------------------------------------------
    #[test]
    fn prop_leader_qualification_theorems(
        level in 1u32..30,
        alignment in -50i32..50,
        is_hostile in any::<bool>(),
    ) {
        let hero = HeroQuestEligibility {
            experience_level: level,
            alignment_record: alignment,
            is_hostile_to_leader: is_hostile,
        };

        let mut state = QuestState::default();
        let res = consult_leader(&mut state, &hero);

        if level < QUEST_MIN_LEVEL || alignment < QUEST_MIN_ALIGNMENT || is_hostile {
            prop_assert!(!is_hero_eligible_for_quest(&hero));
            prop_assert!(res.is_err());
            prop_assert_eq!(state.progress, QuestProgress::Unassigned);
        } else {
            prop_assert!(is_hero_eligible_for_quest(&hero));
            prop_assert!(res.is_ok());
            prop_assert_eq!(state.progress, QuestProgress::Assigned);
        }
    }

    #[test]
    fn prop_nemesis_combat_and_artifact_theorems(
        nemesis_hp in 10u32..500,
        damage in 0u32..1000,
    ) {
        let mut state = QuestState {
            progress: QuestProgress::Assigned,
            artifact_location: ArtifactLocation::HeldByNemesis,
            nemesis_hp,
        };

        let rank_before = quest_progress_rank(state.progress);
        let defeated = attack_nemesis(&mut state, damage);
        let rank_after = quest_progress_rank(state.progress);

        // Theorem: attack_nemesis_monotonic
        prop_assert!(rank_after >= rank_before);

        if damage < nemesis_hp {
            // Theorem: non_fatal_nemesis_retains_artifact
            prop_assert!(!defeated);
            prop_assert_eq!(state.progress, QuestProgress::Assigned);
            prop_assert_eq!(state.artifact_location, ArtifactLocation::HeldByNemesis);
            prop_assert_eq!(state.nemesis_hp, nemesis_hp - damage);
        } else {
            // Theorems: fatal_nemesis_drops_artifact & fatal_nemesis_advances_progress
            prop_assert!(defeated);
            prop_assert_eq!(state.progress, QuestProgress::NemesisDefeated);
            prop_assert_eq!(state.artifact_location, ArtifactLocation::DroppedOnFloor);
            prop_assert_eq!(state.nemesis_hp, 0);

            // Pickup and completion lifecycle
            prop_assert!(pick_up_quest_artifact(&mut state));
            prop_assert_eq!(state.artifact_location, ArtifactLocation::CarriedByHero);
            prop_assert!(return_to_leader_with_artifact(&mut state));
            prop_assert_eq!(state.progress, QuestProgress::Completed);
            prop_assert_eq!(quest_progress_rank(state.progress), 3);
        }
    }
    // -------------------------------------------------------------
    // Theorem: prop_poly_damage_absorption_and_reversion
    // -------------------------------------------------------------
    #[test]
    fn prop_poly_damage_absorption_and_reversion(
        base_hp in 10i32..100,
        poly_hp in 10i32..100,
        damage in 0i32..150,
    ) {
        use netrust_types::{Hero, PolymorphForm};
        use netrust_core::polymorph::{apply_poly_damage, PolyDamageResult};

        let mut hero = Hero { mount: None, quivered_item: None,
            base_hp,
            base_max_hp: base_hp,
            polymorph: Some(PolymorphForm {
                monster_id: 1,
                hp: poly_hp,
                max_hp: poly_hp,
                duration: 100,
            }),
            lycanthropy: None, afflictions: Default::default(), skills: Default::default(),
        };

        let result = apply_poly_damage(&mut hero, damage);
        prop_assert_eq!(hero.base_max_hp, base_hp); // Invariant

        if damage < poly_hp {
            prop_assert!(matches!(result, PolyDamageResult::Absorbed));
            prop_assert!(hero.polymorph.is_some());
            prop_assert_eq!(hero.polymorph.as_ref().unwrap().hp, poly_hp - damage);
            prop_assert_eq!(hero.base_hp, base_hp);
        } else {
            let excess = damage - poly_hp;
            prop_assert!(hero.polymorph.is_none());
            if excess < base_hp {
                let matches_reverted = matches!(result, PolyDamageResult::Reverted { excess_damage } if excess_damage == excess);
                prop_assert!(matches_reverted);
                prop_assert_eq!(hero.base_hp, base_hp - excess);
            } else {
                prop_assert!(matches!(result, PolyDamageResult::Dead));
                prop_assert!(hero.base_hp <= 0);
            }
        }
    }

    // -------------------------------------------------------------
    // Theorem: prop_polypile_count_bounds
    // -------------------------------------------------------------
    #[test]
    fn prop_polypile_count_bounds(
        count in 0usize..50,
        seed in any::<u64>()
    ) {
        use netrust_core::polypile::{polypile_stack, Item};
        use netrust_types::ItemClass;
        let mut items = Vec::new();
        for _ in 0..count {
            items.push(Item {
                name: "sword".to_string(),
                class: ItemClass::Weapon,
            });
        }
        let result = polypile_stack(&items, seed);
        prop_assert!(result.len() <= items.len());
    }

    // -------------------------------------------------------------
    // Theorem: prop_cure_lycanthropy_restores_clean
    // -------------------------------------------------------------
    #[test]
    fn prop_cure_lycanthropy_restores_clean(
        infected in any::<bool>(),
    ) {
        use netrust_types::{Hero, LycanthropyState};
        use netrust_core::polymorph::cure_lycanthropy;
        let mut hero = Hero { mount: None, quivered_item: None,
            base_hp: 10,
            base_max_hp: 10,
            polymorph: None,
            lycanthropy: if infected { Some(LycanthropyState { species: 2, turns_infected: 5 }) } else { None }, afflictions: Default::default(), skills: Default::default(),
        };
        let cured = cure_lycanthropy(&mut hero);
        prop_assert_eq!(cured, infected);
        prop_assert!(hero.lycanthropy.is_none());
    }

    // -------------------------------------------------------------
    // Theorem: is_genocided_species_and_class
    // -------------------------------------------------------------
    #[test]
    fn prop_is_genocided_species_and_class(
        species in ".*",
        glyph in any::<char>(),
        other_species in ".*",
        other_glyph in any::<char>(),
    ) {
        use netrust_types::GenocideRegistry;
        use netrust_core::{is_genocided, apply_genocide};
        use netrust_types::GenocideTarget;

        let mut registry = GenocideRegistry {
            genocided_species: std::collections::HashSet::new(),
            genocided_classes: std::collections::HashSet::new(),
        };
        apply_genocide(&mut registry, GenocideTarget::Species(species.clone()));
        apply_genocide(&mut registry, GenocideTarget::Class(glyph));

        prop_assert!(is_genocided(&registry, &species, glyph));

        if species != other_species && glyph != other_glyph {
            prop_assert!(!is_genocided(&registry, &other_species, other_glyph));
        }
    }

    // -------------------------------------------------------------
    // Theorem: write_with_marker_ink_invariants
    // -------------------------------------------------------------
    #[test]
    fn prop_write_with_marker_ink_invariants(ink in 0u8..=255, cost in 0u8..=255) {
        use netrust_core::write_with_marker;
        let res = write_with_marker(ink, cost);
        if ink >= cost {
            let new_ink = res.unwrap();
            prop_assert_eq!(new_ink, ink - cost);
            if cost > 0 {
                prop_assert!(new_ink < ink);
            }
        } else {
            prop_assert!(res.is_err());
        }
    }

    // -------------------------------------------------------------
    // Theorems: Status Afflictions
    // -------------------------------------------------------------
    #[test]
    fn prop_petrification_countdown_and_cure(
        turns in 1u8..10,
    ) {
        use netrust_core::afflictions::{tick_afflictions, cure_petrification, AfflictionTickResult};
        use netrust_types::{Hero, AfflictionState, PetrificationState};
        let mut hero = Hero { mount: None, quivered_item: None,
            base_hp: 10,
            base_max_hp: 10,
            polymorph: None,
            lycanthropy: None,
            afflictions: AfflictionState {
                petrification: Some(PetrificationState { turns_remaining: turns }),
                ..Default::default()
            },
            skills: Default::default(),
        };

        // Decrement by ticks
        for _ in 0..turns - 1 {
            let res = tick_afflictions(&mut hero);
            prop_assert_eq!(res, AfflictionTickResult::Survived);
        }

        let final_res = tick_afflictions(&mut hero);
        prop_assert_eq!(final_res, AfflictionTickResult::StoneDeath);

        // Reset and cure
        hero.afflictions.petrification = Some(PetrificationState { turns_remaining: turns });
        cure_petrification(&mut hero);
        prop_assert!(hero.afflictions.petrification.is_none());

        let cured_res = tick_afflictions(&mut hero);
        prop_assert_eq!(cured_res, AfflictionTickResult::Survived);
    }

    #[test]
    fn prop_sliming_countdown_and_cure(
        turns in 1u8..10,
    ) {
        use netrust_core::afflictions::{tick_afflictions, cure_sliming, AfflictionTickResult};
        use netrust_types::{Hero, AfflictionState, SlimingState};
        let mut hero = Hero { mount: None, quivered_item: None,
            base_hp: 10,
            base_max_hp: 10,
            polymorph: None,
            lycanthropy: None,
            afflictions: AfflictionState {
                sliming: Some(SlimingState { turns_remaining: turns }),
                ..Default::default()
            },
            skills: Default::default(),
        };

        // Decrement by ticks
        for _ in 0..turns - 1 {
            let res = tick_afflictions(&mut hero);
            prop_assert_eq!(res, AfflictionTickResult::Survived);
        }

        let final_res = tick_afflictions(&mut hero);
        prop_assert_eq!(final_res, AfflictionTickResult::SlimeDeath);

        // Reset and cure
        hero.afflictions.sliming = Some(SlimingState { turns_remaining: turns });
        cure_sliming(&mut hero);
        prop_assert!(hero.afflictions.sliming.is_none());

        let cured_res = tick_afflictions(&mut hero);
        prop_assert_eq!(cured_res, AfflictionTickResult::Survived);
    }

    // -------------------------------------------------------------
    // Theorems: Weapon Skills
    // -------------------------------------------------------------
    #[test]
    fn prop_skill_bonuses_monotonic(
        _dummy in any::<bool>(),
    ) {
        use netrust_core::skills::{skill_to_hit_bonus, skill_damage_bonus};
        use netrust_types::SkillLevel;

        let levels = [
            SkillLevel::Unskilled,
            SkillLevel::Basic,
            SkillLevel::Skilled,
            SkillLevel::Expert,
        ];

        for i in 0..levels.len() {
            for j in i..levels.len() {
                let li = levels[i];
                let lj = levels[j];

                prop_assert!(li <= lj);
                prop_assert!(skill_to_hit_bonus(lj) >= skill_to_hit_bonus(li));
                prop_assert!(skill_damage_bonus(lj) >= skill_damage_bonus(li));
            }
        }
    }

    #[test]
    fn prop_enhance_skill_slot_conservation(
        slots in 1u8..10,
    ) {
        use netrust_core::skills::enhance_skill;
        use netrust_types::{SkillTree, SkillClass, SkillLevel};

        let mut tree = SkillTree {
            skills: std::collections::HashMap::new(),
            available_slots: slots,
        };

        let skill = SkillClass::LongSword;

        // Unskilled -> Basic
        let res = enhance_skill(&mut tree, skill);
        prop_assert!(res.is_ok());
        prop_assert_eq!(tree.available_slots, slots - 1);
        prop_assert_eq!(tree.skills.get(&skill), Some(&SkillLevel::Basic));

        // Basic -> Skilled
        if tree.available_slots > 0 {
            let slots_before = tree.available_slots;
            let res2 = enhance_skill(&mut tree, skill);
            prop_assert!(res2.is_ok());
            prop_assert_eq!(tree.available_slots, slots_before - 1);
            prop_assert_eq!(tree.skills.get(&skill), Some(&SkillLevel::Skilled));

            // Skilled -> Expert
            if tree.available_slots > 0 {
                let slots_before = tree.available_slots;
                let res3 = enhance_skill(&mut tree, skill);
                prop_assert!(res3.is_ok());
                prop_assert_eq!(tree.available_slots, slots_before - 1);
                prop_assert_eq!(tree.skills.get(&skill), Some(&SkillLevel::Expert));

                // Expert -> Cannot advance
                if tree.available_slots > 0 {
                    let slots_before = tree.available_slots;
                    let res4 = enhance_skill(&mut tree, skill);
                    prop_assert!(res4.is_err());
                    prop_assert_eq!(tree.available_slots, slots_before);
                }
            }
        }
    }
}

use netrust_core::ranged::{can_mount, effective_movement_cost, resolve_projectile_impact};

proptest! {
    #[test]
    fn prop_effective_movement_cost_monotonic(unmounted_cost in 1u32..1000, mount_cost in proptest::option::of(1u32..1000)) {
        let cost = effective_movement_cost(unmounted_cost, mount_cost);
        prop_assert!(cost <= unmounted_cost);
    }

    #[test]
    fn prop_can_mount_requires_both_tame_and_saddle(is_tame in any::<bool>(), has_saddle in any::<bool>()) {
        let result = can_mount(is_tame, has_saddle);
        prop_assert_eq!(result, is_tame && has_saddle);
    }

    #[test]
    fn prop_projectile_impact_breakage(break_prob in 0u8..=100, roll in 0u8..100) {
        let result = resolve_projectile_impact(break_prob, roll);
        prop_assert_eq!(result, roll < break_prob);
    }
}

prop_compose! {
    fn arb_trap_type()(idx in 0..13) -> netrust_types::TrapType {
        match idx {
            0 => netrust_types::TrapType::Arrow,
            1 => netrust_types::TrapType::Dart,
            2 => netrust_types::TrapType::RockFall,
            3 => netrust_types::TrapType::Pit,
            4 => netrust_types::TrapType::SpikedPit,
            5 => netrust_types::TrapType::Teleport,
            6 => netrust_types::TrapType::Fire,
            7 => netrust_types::TrapType::LevelTeleport,
            8 => netrust_types::TrapType::Polymorph,
            9 => netrust_types::TrapType::AntiMagic,
            10 => netrust_types::TrapType::SleepingGas,
            11 => netrust_types::TrapType::Rust,
            _ => netrust_types::TrapType::Web,
        }
    }
}

prop_compose! {
    fn arb_trap_state()(idx in 0..3) -> netrust_types::TrapState {
        match idx {
            0 => netrust_types::TrapState::Hidden,
            1 => netrust_types::TrapState::Revealed,
            _ => netrust_types::TrapState::Disarmed,
        }
    }
}

prop_compose! {
    fn arb_trap_record()(trap_type in arb_trap_type(), state in arb_trap_state()) -> netrust_types::TrapRecord {
        netrust_types::TrapRecord {
            id: 0,
            trap_type,
            state,
            coord: netrust_core::Coord::new_unchecked(0, 0),
        }
    }
}

proptest! {
    // Reference written directly from NetHack 3.7 C: trap.c:1061 floor_trigger,
    // dotrap trap.c:2996-3046 (check_in_air; already_seen && !rn2(5) escape).
    #[test]
    fn prop_can_trigger_trap_matches_c_reference(
        trap in arb_trap_record(),
        flying in proptest::bool::ANY,
        roll in 0u32..100,
    ) {
        use netrust_types::{TrapState, TrapType};
        let floor_trigger = matches!(
            trap.trap_type,
            TrapType::Arrow | TrapType::Dart | TrapType::RockFall | TrapType::Pit
                | TrapType::SpikedPit | TrapType::Fire | TrapType::SleepingGas | TrapType::Rust
        );
        let disarmed = trap.state == TrapState::Disarmed;
        let avoided_in_air = floor_trigger && flying;
        let escaped_seen = trap.state == TrapState::Revealed && roll % 5 == 0;
        let expected = !(disarmed || avoided_in_air || escaped_seen);
        prop_assert_eq!(netrust_core::traps::is_floor_trap(trap.trap_type), floor_trigger);
        prop_assert_eq!(netrust_core::traps::can_trigger_trap(&trap, flying, roll), expected);
    }

    #[test]
    fn prop_disarmed_trap_never_triggers(
        mut trap in arb_trap_record(),
        is_flying in proptest::bool::ANY,
        roll in 0u32..100,
    ) {
        trap.state = netrust_types::TrapState::Disarmed;
        prop_assert!(!netrust_core::traps::can_trigger_trap(&trap, is_flying, roll));
    }

    #[test]
    fn prop_trigger_trap_reveals_hidden(mut trap in arb_trap_record(), roll in 0u32..100) {
        trap.state = netrust_types::TrapState::Hidden;
        let triggered = netrust_core::traps::trigger_trap(&mut trap, false, roll);
        prop_assert!(triggered.is_some());
        prop_assert_eq!(trap.state, netrust_types::TrapState::Revealed);
    }

    #[test]
    fn prop_disarm_trap_transitions_to_disarmed(mut trap in arb_trap_record()) {
        let is_already_disarmed = trap.state == netrust_types::TrapState::Disarmed;
        let res = netrust_core::traps::disarm_trap(&mut trap);
        if is_already_disarmed {
            prop_assert!(!res);
        } else {
            prop_assert!(res);
        }
        prop_assert_eq!(trap.state, netrust_types::TrapState::Disarmed);
    }
}

prop_compose! {
    fn arb_conduct_tracker()(
        pacifist in any::<bool>(),
        vegan in any::<bool>(),
        vegetarian in any::<bool>(),
        atheist in any::<bool>(),
        illiterate in any::<bool>(),
        genocideless in any::<bool>(),
        polypileless in any::<bool>(),
        wishless in any::<bool>(),
    ) -> netrust_types::ConductTracker {
        netrust_types::ConductTracker {
            pacifist, vegan, vegetarian, atheist, illiterate, genocideless, polypileless, wishless
        }
    }
}

proptest! {
    // -------------------------------------------------------------
    // Theorem: prop_corpse_tainted_threshold
    // -------------------------------------------------------------
    #[test]
    fn prop_corpse_tainted_threshold(age in 0u32..1000, rot_threshold in 0u32..1000) {
        prop_assert_eq!(
            netrust_core::nutrition::is_corpse_tainted(age, rot_threshold),
            age > rot_threshold
        );
    }

    // -------------------------------------------------------------
    // Theorem: prop_cannibalism_same_race
    // -------------------------------------------------------------
    #[test]
    fn prop_cannibalism_same_race(corpse_race in "[a-z]+", hero_race in "[a-z]+") {
        let expected = corpse_race == hero_race;
        prop_assert_eq!(
            netrust_core::nutrition::is_cannibalism(&corpse_race, &hero_race),
            expected
        );
    }

    // -------------------------------------------------------------
    // Theorem: prop_conduct_irreversibility
    // -------------------------------------------------------------
    #[test]
    fn prop_conduct_irreversibility(tracker_base in arb_conduct_tracker()) {
        let mut t1 = tracker_base.clone();
        netrust_core::conducts::record_kill(&mut t1);
        if !tracker_base.pacifist { prop_assert_eq!(t1.pacifist, false); }

        let mut t2 = tracker_base.clone();
        netrust_core::conducts::record_eat_meat(&mut t2);
        if !tracker_base.vegan { prop_assert_eq!(t2.vegan, false); }
        if !tracker_base.vegetarian { prop_assert_eq!(t2.vegetarian, false); }

        let mut t3 = tracker_base.clone();
        netrust_core::conducts::record_read(&mut t3);
        if !tracker_base.illiterate { prop_assert_eq!(t3.illiterate, false); }

        let mut t4 = tracker_base.clone();
        netrust_core::conducts::record_altar_action(&mut t4);
        if !tracker_base.atheist { prop_assert_eq!(t4.atheist, false); }

        let mut t5 = tracker_base.clone();
        netrust_core::conducts::record_wish(&mut t5);
        if !tracker_base.wishless { prop_assert_eq!(t5.wishless, false); }

        let mut t6 = tracker_base.clone();
        netrust_core::conducts::record_polypile(&mut t6);
        if !tracker_base.polypileless { prop_assert_eq!(t6.polypileless, false); }
    }
}

//! Property-Based Tests validating the Lean 4 Theorems in Safe Rust.
//!
//! Every property test here corresponds to a machine-checked theorem in `NetMechanics`.

use netrust_core::{
    apply_erosion, apply_vorpal_strike, arm_bonus, attack_hits, attack_nemesis,
    branch_entrance_depth, branch_max_depth, buy_price, calculate_damage, calculate_encumbrance,
    calculate_summon_count, calculate_tournament_score, can_detect_monster, can_see_tile,
    cast_spell, choose_pet_goal, clamp_favor, consecrate_water, consult_leader,
    corrupt_buc_on_death, create_ghost_hp, decide_tactical_action, destroy_drawbridge,
    dilute_potion, dip_water, dmgval, enchant_armor, enchant_weapon, enter_branch, exit_branch,
    feed_pet, find_ac, hero_damage_after_ac, hunger_of_nutrition, hunger_tier, identify_fully,
    interact_with_occupant, is_candelabrum_ready, is_elbereth_ward_active,
    is_hero_eligible_for_quest, is_hp_critical, is_sanctum_accessible, is_valid_bones_level,
    learn_buc, learn_type, luck_decay_period, mana_cost, mattacku_die, mbag_explodes, melee_damage,
    mhitm_to_hit, mix_alchemy, monster_attack_damage, monster_attack_hits, monster_hit_damage,
    monster_to_hit_value, mysterious_force, offer_amulet_on_high_altar, onscary_exempt,
    peace_minded, pet_tile_steppable, pick_up_quest_artifact, priest_donation_outcome,
    priest_donation_quan, priest_suggested_donation, priest_uncurse, promote_pet,
    protection_purchase_count, protection_purchase_step, push_boulder, quest_progress_rank,
    recharge_wand, reflect, resisted, resolve_breath_damage, resolve_gaze, resolve_sacrifice,
    return_to_leader_with_artifact, rub_lamp, sell_price, step_luck_decay, step_ray, step_ritual,
    swap_displacement, tick_light_fuel, tick_prayer_timeout, to_hit_value, toggle_drawbridge,
    uncurse, weapon_damage_die, zap_hit, zap_wand, Alignment, ArtifactLocation, AscensionOutcome,
    BagCheckItem, BagCheckKind, BeamRay, BranchCoord, BranchId, BreathType, Buc, CandelabrumState,
    Combatant, Coord, DilutionState, Direction, DivineState, DonationOutcome, DoorState,
    DrawbridgeState, DrawbridgeTransition, DungeonDepth, EnchantOutcome, EncumbranceTier,
    Engraving, EngravingMedium, FormStats, GazeEffect, GazeType, HeroInteraction,
    HeroQuestEligibility, Intrinsics, InvocationStep, KnowledgeLevel, LightSource, MetricState,
    MysteriousForceOutcome, PetFamily, PetGoal, PetSpeciesTier, PolyEntity, PushOutcome,
    QuestProgress, QuestState, RechargeResult, RitualProgress, RubResult, SchedulerState,
    SpellKind, StepAction, StepResult, SurfaceOrientation, TacticalAction, TacticalContext, Tile,
    Velocity, WandCharges, WaterType, MAX_DIVINE_PROTECTION, NORMAL_SPEED, QUEST_MIN_ALIGNMENT,
    QUEST_MIN_LEVEL, REQUIRED_CANDLES,
};
use netrust_types::{Attack, AttackType, DamageType};
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

/// Reference for `seffect_enchant_armor` (read.c:1115-1200), written from the C
/// with i64 math. `None` = evaporated.
fn ref_enchant_armor(
    spe: i8,
    buc: Buc,
    special: bool,
    magical: bool,
    rn2_s: u32,
    gain_roll: u32,
) -> Option<i8> {
    let scursed = buc == Buc::Cursed;
    let sblessed = buc == Buc::Blessed;
    let otmp_spe = i64::from(spe);
    let mut s = if scursed { -otmp_spe } else { otmp_spe };
    if s > (if special { 5 } else { 3 }) && i64::from(rn2_s).min(s - 1) != 0 {
        return None;
    }
    s = (4 - s) / 2;
    if special {
        s += 1;
    }
    if !magical {
        s += 1;
    }
    if sblessed {
        s += 1;
    }
    if s <= 0 {
        s = 0;
        if otmp_spe > 0 && i64::from(gain_roll).min(otmp_spe - 1) == 0 {
            s = 1;
        }
    } else {
        s = i64::from(gain_roll).clamp(1, s);
    }
    if s > 11 {
        s = 11;
    }
    if scursed {
        s = -s;
    }
    Some((otmp_spe + s).clamp(-128, 127) as i8)
}

/// Reference for `seffect_enchant_weapon` (read.c:1667) + `chwepon`
/// (wield.c:999-1000). `None` = evaporated.
fn ref_enchant_weapon(spe: i8, buc: Buc, rn2_3: u32, gain_roll: u32) -> Option<i8> {
    let spe = i64::from(spe);
    let amount = if buc == Buc::Cursed {
        -1
    } else if spe >= 9 {
        i64::from(i64::from(gain_roll).min(spe - 1) == 0)
    } else if buc == Buc::Blessed {
        i64::from(gain_roll).clamp(1, 3 - spe / 3)
    } else {
        1
    };
    if ((spe > 5 && amount >= 0) || (spe < -5 && amount < 0)) && rn2_3.min(2) != 0 {
        return None;
    }
    Some((spe + amount).clamp(-128, 127) as i8)
}

fn arb_bag_kind() -> impl Strategy<Value = BagCheckKind> {
    prop_oneof![
        Just(BagCheckKind::BagOfHolding),
        (-2i32..4).prop_map(|charges| BagCheckKind::BagOfTricks { charges }),
        (-2i32..4).prop_map(|charges| BagCheckKind::WandOfCancellation { charges }),
        Just(BagCheckKind::Other),
    ]
}

fn arb_bag_tree(depth: u32) -> BoxedStrategy<BagCheckItem> {
    let leaf = arb_bag_kind().prop_map(|kind| BagCheckItem {
        kind,
        children: vec![],
    });
    leaf.prop_recursive(depth, 12, 3, |inner| {
        (arb_bag_kind(), proptest::collection::vec(inner, 0..3))
            .prop_map(|(kind, children)| BagCheckItem { kind, children })
    })
    .boxed()
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
    // C reference: rnd.c d(n, x) = n + sum of n RND(x) draws (each 1..=x),
    // used by hitmu (mhitu.c:1187) and mdamagem (mhitm.c:1025).
    // -------------------------------------------------------------
    #[test]
    fn prop_monster_attack_damage_matches_c_dice(
        n in 0u8..10,
        d in 0u8..80,
        rolls in proptest::collection::vec(0u32..100, 0..12)
    ) {
        let attack = Attack { at: AttackType::Claw, ad: DamageType::Phys, n, d };
        let expected: u32 = if d == 0 {
            0
        } else {
            (0..n as usize)
                .map(|i| rolls.get(i).copied().unwrap_or(1).clamp(1, d as u32))
                .sum()
        };
        let dmg = monster_attack_damage(&attack, &rolls);
        prop_assert_eq!(dmg, expected);
        if d > 0 {
            prop_assert!(dmg >= n as u32 && dmg <= n as u32 * d as u32);
        }
    }

    // -------------------------------------------------------------
    // C reference: mhitm.c:321 tmp = find_mac(mdef) + m_lev (no +10),
    // mhitm.c:441 strike = tmp > rnd(20 + i); mhitu.c:794 same die.
    // -------------------------------------------------------------
    #[test]
    fn prop_mhitm_to_hit_matches_c(
        m_lev in 0i32..50,
        def_ac in -40i32..20,
        i in 0u32..6,
        roll in 0u32..40
    ) {
        let (tmp, die) = mhitm_to_hit(m_lev, def_ac, i);
        prop_assert_eq!(tmp, def_ac + m_lev);
        prop_assert_eq!(die, 20 + i);
        prop_assert_eq!(mattacku_die(i), 20 + i);
        let dieroll = roll.clamp(1, die) as i32;
        prop_assert_eq!(monster_attack_hits(tmp, die, roll), tmp > dieroll);
    }

    // -------------------------------------------------------------
    // C reference: hitmu mhitu.c:1187 dmg = d(n,d); mhitm_ad_fire/cold
    // (uhitm.c:2521/2626) zero it under resistance; mhitu.c:1208
    // `if (dmg && u.uac < 0) dmg -= rnd(-u.uac), min 1`.
    // -------------------------------------------------------------
    #[test]
    fn prop_monster_hit_damage_matches_c(
        n in 1u8..8,
        d in 1u8..12,
        rolls in proptest::collection::vec(1u32..12, 8),
        fire in any::<bool>(),
        fire_res in any::<bool>(),
        hero_ac in proptest::option::of(-20i32..11),
        absorb in 0u32..30
    ) {
        let ad = if fire { DamageType::Fire } else { DamageType::Phys };
        let attack = Attack { at: AttackType::Touch, ad, n, d };
        let mut intr = Intrinsics::empty();
        intr.fire_resistance = fire_res;
        let res = resisted(ad, &intr);
        prop_assert_eq!(res, fire && fire_res);
        let mut dmg: u32 = (0..n as usize).map(|i| rolls[i].clamp(1, d as u32)).sum();
        if res {
            dmg = 0;
        }
        if let Some(ac) = hero_ac {
            if dmg > 0 && ac < 0 {
                dmg = dmg.saturating_sub(absorb.clamp(1, (-ac) as u32)).max(1);
            }
        }
        let got = monster_hit_damage(&attack, &rolls, res, hero_ac.map(|ac| (ac, absorb)));
        prop_assert_eq!(got, dmg);
        prop_assert_eq!(got == 0, res);
    }

    // -------------------------------------------------------------
    // C reference: zap.c:4705 zap_hit(u.uac, 0) for a breath ray at the hero.
    // -------------------------------------------------------------
    #[test]
    fn prop_zap_hit_matches_c(
        ac in -30i32..15,
        chance in 0u32..25,
        rnd10 in 0u32..15,
        ac_roll in 0u32..40
    ) {
        let c = chance.min(19) as i32;
        let expected = if c == 0 {
            (rnd10.clamp(1, 10) as i32) < ac
        } else {
            let acv = if ac >= 0 { ac } else { -(ac_roll.clamp(1, (-ac) as u32) as i32) };
            3 - c < acv
        };
        prop_assert_eq!(zap_hit(ac, chance, rnd10, ac_roll), expected);
    }

    // -------------------------------------------------------------
    // C reference: weapon.c:216-293 (dmgval) and uhitm.c:847 (bare hands / martial arts).
    // -------------------------------------------------------------
    #[test]
    fn prop_dmgval_matches_c_rule(
        weapon in proptest::option::of((0u32..50, 0u32..50)),
        target_large in any::<bool>(),
        martial_arts in any::<bool>(),
        roll in 0u32..100,
    ) {
        let expected_die = match weapon {
            Some((small, large)) => {
                if target_large {
                    large
                } else {
                    small
                }
            }
            None => {
                if martial_arts {
                    4
                } else {
                    2
                }
            }
        };
        let die = weapon_damage_die(weapon, target_large, martial_arts);
        prop_assert_eq!(die, expected_die);

        let expected_dmg = if expected_die == 0 {
            0
        } else {
            roll.clamp(1, expected_die)
        };
        let got = dmgval(weapon, target_large, martial_arts, roll);
        prop_assert_eq!(got, expected_dmg);
        if expected_die > 0 {
            prop_assert!(got >= 1 && got <= expected_die);
        }
    }

    // -------------------------------------------------------------
    // C reference: hack.h:1526-1528 (ARM_BONUS) and do_wear.c:2473-2507 (find_ac).
    // -------------------------------------------------------------
    #[test]
    fn prop_find_ac_matches_c_reference(
        base_ac in -20i32..=30,
        protection in -10i32..=50,
        worn in proptest::collection::vec(
            (0i32..=15, -10i32..=15, 0u8..=5),
            0..=7,
        ),
    ) {
        fn c_reference_arm_bonus(a_ac: i32, spe: i32, erosion: u8) -> i32 {
            let ero = if a_ac > 0 {
                (erosion as i32).min(a_ac)
            } else {
                0
            };
            a_ac + spe - ero
        }

        fn c_reference_find_ac(
            base_ac: i32,
            worn: &[(i32, i32, u8)],
            protection: i32,
        ) -> i32 {
            let mut uac = base_ac;
            for &(a_ac, spe, erosion) in worn {
                uac -= c_reference_arm_bonus(a_ac, spe, erosion);
            }
            uac -= protection;
            if uac.abs() > 99 {
                uac.signum() * 99
            } else {
                uac
            }
        }

        for &(a, s, e) in &worn {
            prop_assert_eq!(arm_bonus(a, s, e), c_reference_arm_bonus(a, s, e));
        }

        let got = find_ac(base_ac, &worn, protection);
        let expected = c_reference_find_ac(base_ac, &worn, protection);
        prop_assert_eq!(got, expected);
        prop_assert!((-99..=99).contains(&got));
    }

    // -------------------------------------------------------------
    // Theorem: peace_minded_matches_c_reference
    // -------------------------------------------------------------
    #[test]
    fn prop_peace_minded_matches_c_reference(
        arch_peaceful in any::<bool>(),
        always_hostile in any::<bool>(),
        mal in -2..=2i32,
        ual in -2..=2i32,
        rec in -20..=20i32,
        roll in 0..1000u32,
    ) {
        fn c_reference_peace_minded(
            arch_peaceful: bool,
            always_hostile: bool,
            mal: i32,
            ual: i32,
            rec: i32,
            roll: u32,
        ) -> bool {
            if arch_peaceful {
                return true;
            }
            if always_hostile {
                return false;
            }
            if mal.signum() != ual.signum() {
                return false;
            }
            let a = (16 + rec.max(-15)) as u32;
            let b = (2 + mal.abs()) as u32;
            let peaceful_outcomes = a.saturating_sub(1) * b.saturating_sub(1);
            roll < peaceful_outcomes
        }

        let got = peace_minded(arch_peaceful, always_hostile, mal, ual, rec, roll);
        let expected = c_reference_peace_minded(arch_peaceful, always_hostile, mal, ual, rec, roll);
        prop_assert_eq!(got, expected);
    }

    // -------------------------------------------------------------
    // Theorem: onscary_exempt_and_elbereth
    // -------------------------------------------------------------
    #[test]
    fn prop_onscary_exempt_and_elbereth(
        is_human in any::<bool>(),
        is_minotaur in any::<bool>(),
        is_shk in any::<bool>(),
        is_rider in any::<bool>(),
        is_blind in any::<bool>(),
        is_covetous in any::<bool>(),
        is_peaceful in any::<bool>(),
    ) {
        let exempt = onscary_exempt(is_human, is_minotaur, is_shk, is_rider);
        prop_assert_eq!(exempt, is_human || is_minotaur || is_shk || is_rider);

        let elbereth = Engraving::new("Elbereth", EngravingMedium::Burned);
        let active = is_elbereth_ward_active(
            Some(&elbereth),
            is_blind,
            is_covetous,
            is_peaceful,
            exempt,
        );

        if is_blind || is_covetous || is_peaceful || exempt {
            prop_assert!(!active);
        } else {
            prop_assert!(active);
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
    // Theorem: boh_cannot_contain_boh / depth_zero_boh_explodes /
    // cancellation_wand_explodes (pickup.c:2488 mbag_explodes)
    // -------------------------------------------------------------
    #[test]
    fn prop_mbag_explodes_matches_c_reference(
        tree in arb_bag_tree(3),
        rolls in proptest::collection::vec(any::<u32>(), 64),
    ) {
        // Reference written directly from pickup.c:2488-2507; `rolls` are
        // consumed in C draw order, reduced to rn2(n) range by `% n`.
        fn reference(o: &BagCheckItem, depth: u32, rolls: &mut impl Iterator<Item = u32>) -> bool {
            let (otyp_cancel_or_tricks, spe, is_mbag, is_cancel) = match o.kind {
                BagCheckKind::BagOfHolding => (false, 0, true, false),
                BagCheckKind::BagOfTricks { charges } => (true, charges, true, false),
                BagCheckKind::WandOfCancellation { charges } => (true, charges, false, true),
                BagCheckKind::Other => (false, 0, false, false),
            };
            if otyp_cancel_or_tricks && spe <= 0 {
                return false;
            }
            if (is_mbag || is_cancel) && {
                let n = 1u32 << (if depth > 7 { 7 } else { depth });
                rolls.next().unwrap_or(0) % n <= depth
            } {
                return true;
            }
            for c in &o.children {
                if reference(c, depth + 1, rolls) {
                    return true;
                }
            }
            false
        }
        let mut it_ref = rolls.clone().into_iter();
        let expected = reference(&tree, 0, &mut it_ref);
        let mut it = rolls.clone().into_iter();
        let mut bounds_ok = true;
        let got = mbag_explodes(&tree, 0, &mut |n| {
            bounds_ok &= n.is_power_of_two() && n <= 128;
            it.next().unwrap_or(0) % n
        });
        prop_assert_eq!(got, expected);
        prop_assert!(bounds_ok);
        // Same number of draws as C.
        prop_assert_eq!(it.count(), it_ref.count());
    }

    #[test]
    fn prop_depth_zero_boh_always_explodes(roll in any::<u32>()) {
        let boh = BagCheckItem { kind: BagCheckKind::BagOfHolding, children: vec![] };
        prop_assert!(mbag_explodes(&boh, 0, &mut |_| roll));
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
        let (after, _) = entity.apply_damage(damage, false);
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
        let (after, dead) = entity.apply_damage(poly_hp + extra, false);
        prop_assert!(!after.is_polymorphed());
        // C rehumanize: excess discarded, base HP untouched, never fatal.
        prop_assert_eq!(after.base_form.hp, 20);
        prop_assert!(!dead);
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
    // Theorems: enchant_armor_* & enchant_weapon_* (read.c:1115, wield.c:999)
    // -------------------------------------------------------------
    #[test]
    fn prop_enchant_armor_matches_c_reference(
        spe in any::<i8>(),
        buc in arb_buc(),
        special in proptest::bool::ANY,
        magical in proptest::bool::ANY,
        evap in 0u32..300,
        gain in 0u32..300,
    ) {
        let expected = match ref_enchant_armor(spe, buc, special, magical, evap, gain) {
            Some(v) => EnchantOutcome::Changed(v),
            None => EnchantOutcome::Evaporated,
        };
        prop_assert_eq!(enchant_armor(spe, buc, special, magical, evap, gain), expected);
    }

    #[test]
    fn prop_enchant_weapon_matches_c_reference(
        spe in any::<i8>(),
        buc in arb_buc(),
        evap in 0u32..10,
        gain in 0u32..300,
    ) {
        let expected = match ref_enchant_weapon(spe, buc, evap, gain) {
            Some(v) => EnchantOutcome::Changed(v),
            None => EnchantOutcome::Evaporated,
        };
        prop_assert_eq!(enchant_weapon(spe, buc, evap, gain), expected);
    }

    #[test]
    fn prop_enchant_safe_at_or_below_limit(
        spe in -3i8..=5,
        buc in arb_buc(),
        special in proptest::bool::ANY,
        magical in proptest::bool::ANY,
        evap in any::<u32>(),
        gain in any::<u32>(),
    ) {
        // enchant_weapon_safe_le_limit: weapon at spe <= 5 never evaporates.
        prop_assert_ne!(enchant_weapon(spe, buc, evap, gain), EnchantOutcome::Evaporated);
        // enchant_armor_safe_le_limit: armor at spe <= 3 (5 special) never evaporates.
        if spe <= if special { 5 } else { 3 } {
            prop_assert_ne!(
                enchant_armor(spe, buc, special, magical, evap, gain),
                EnchantOutcome::Evaporated
            );
        }
    }

    #[test]
    fn prop_enchant_weapon_increases_below_limit(
        spe in -100i8..=5,
        blessed in proptest::bool::ANY,
        evap in any::<u32>(),
        gain in any::<u32>(),
    ) {
        let buc = if blessed { Buc::Blessed } else { Buc::Uncursed };
        match enchant_weapon(spe, buc, evap, gain) {
            EnchantOutcome::Changed(v) => prop_assert!(v > spe),
            EnchantOutcome::Evaporated => prop_assert!(false, "evaporated below limit"),
        }
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
    // Theorems: recharge_safe_first, recharge_explodes_iff, recharge_explodes_at_cap
    // Reference written from NetHack read.c:737-794.
    // -------------------------------------------------------------
    #[test]
    fn prop_recharge_theorems(
        charges in 0u32..20,
        recharges in 0u32..10,
        wishing in proptest::bool::ANY,
        buc_i in 0u8..3,
        directional in proptest::bool::ANY,
        roll_343 in 0u32..343,
        rn5 in 0u32..5,
        rnd_roll in 1u32..16,
        wand_blessed in proptest::bool::ANY,
    ) {
        let buc = [Buc::Cursed, Buc::Uncursed, Buc::Blessed][buc_i as usize];
        let lim: u32 = if wishing { 1 } else if directional { 8 } else { 15 };
        let w = WandCharges { charges, recharges };
        let res = recharge_wand(w, buc, lim, wishing, wand_blessed, roll_343, rn5, rnd_roll);
        // Reference from read.c:737-794.
        let n = recharges.min(7);
        let explode = n > 0 && (wishing || n * n * n > roll_343);
        let expected = if explode {
            RechargeResult::Exploded
        } else if buc == Buc::Cursed {
            RechargeResult::Success(WandCharges {
                charges: if wand_blessed { charges } else { 0 },
                recharges: recharges + 1,
            })
        } else {
            let mut amt = if lim == 1 { 1 } else { (lim - 4) + rn5 };
            if buc != Buc::Blessed {
                amt = rnd_roll.min(amt).max(1);
            }
            let spe = std::cmp::max(charges + 1, amt);
            if wishing && spe > 3 {
                RechargeResult::Exploded
            } else {
                RechargeResult::Success(WandCharges { charges: spe, recharges: recharges + 1 })
            }
        };
        prop_assert_eq!(res.clone(), expected);
        if recharges == 0 && !wishing {
            prop_assert!(matches!(res, RechargeResult::Success(_)));
        }
        if recharges >= 7 {
            prop_assert_eq!(res, RechargeResult::Exploded);
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
        // C: `if (rn2(5)) curse(otmp);` rn2(5) in 0..=4; Amulet/invocation items always cursed.
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
    // Theorems: sell_le_buy_price, buy_price_antitone_cha (shk.c get_cost / set_cost)
    // -------------------------------------------------------------
    #[test]
    fn prop_buy_price_matches_c_reference(
        base in 0u32..5000,
        cha in -3i32..30,
        dunce in any::<bool>(),
        unid in any::<bool>(),
        artifact in any::<bool>(),
        angry in any::<bool>(),
    ) {
        prop_assert_eq!(
            buy_price(base, cha, dunce, unid, artifact, angry),
            c_get_cost(base, cha, dunce, unid, artifact, angry)
        );
    }

    #[test]
    fn prop_sell_price_matches_c_reference(
        base in 0u32..5000,
        dunce in any::<bool>(),
        lowball in any::<bool>(),
    ) {
        prop_assert_eq!(sell_price(base, dunce, lowball), c_set_cost(base, dunce, lowball));
    }

    #[test]
    fn prop_price_identification_theorems(
        base in 0u32..5000,
        cha in -3i32..30,
        dunce in any::<bool>(),
        unid in any::<bool>(),
        artifact in any::<bool>(),
        angry in any::<bool>(),
        lowball in any::<bool>(),
    ) {
        // sell_le_buy_price: no arbitrage for any CHA / surcharge combination.
        let buy = buy_price(base, cha, dunce, unid, artifact, angry);
        prop_assert!(sell_price(base, dunce, lowball) <= buy);
        // buy_price_antitone_cha: higher CHA never raises the buy price.
        prop_assert!(buy_price(base, cha + 1, dunce, unid, artifact, angry) <= buy);
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
            2 => PetSpeciesTier::LargeDog,
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
        prop_assert_eq!(promote_pet(PetSpeciesTier::LargeDog, l1), PetSpeciesTier::LargeDog);
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
    // Theorems: priest_protection_bounded, priest_protection_monotonic,
    // priest_protection_insufficient (priest.c:637-699)
    // -------------------------------------------------------------
    #[test]
    fn prop_priest_protection_theorems(
        cur in 0u32..=20,
        level_peak in 0u32..=30,
        cheapskate in 0u32..=5,
        rn2_101 in 0u32..101,
        gold in 0u32..=200_000,
        offer in 0u32..=200_000,
        rolls in proptest::collection::vec(0u32..40, 0..64),
    ) {
        let suggested = priest_suggested_donation(level_peak, cheapskate, rn2_101);
        prop_assert_eq!(suggested, level_peak.max(1) * (rn2_101 + 150 + cheapskate * 40));
        let quan = priest_donation_quan(gold, suggested);
        prop_assert_eq!(quan, (gold / (suggested * 3)).max(1));

        let outcome = priest_donation_outcome(offer, suggested, quan, gold.saturating_sub(offer));
        let mut prot = cur;
        if outcome == DonationOutcome::Protection {
            let n = protection_purchase_count(offer, suggested);
            prop_assert_eq!(n, offer / (2 * suggested));
            for i in 0..n as usize {
                let r = rolls.get(i).copied().unwrap_or(0);
                prop_assert_eq!(protection_purchase_step(prot, r), c_ublessed_step(prot, r));
                prot = protection_purchase_step(prot, r);
            }
        }
        // Bounded by the hard cap 20, monotonic.
        prop_assert!(prot <= MAX_DIVINE_PROTECTION);
        prop_assert!(cur <= prot);
        // Below the protection band nothing changes.
        if offer < 2 * suggested * quan {
            prop_assert!(outcome != DonationOutcome::Protection);
        }
    }

    #[test]
    fn prop_priest_donation_outcome_matches_c_reference(
        offer in 0u32..=100_000,
        suggested in 150u32..=20_000,
        quan in 1u32..=50,
        gold_after in 0u32..=200_000,
    ) {
        prop_assert_eq!(
            priest_donation_outcome(offer, suggested, quan, gold_after),
            c_priest_band(offer, suggested, quan, gold_after)
        );
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
    // Theorem: mysterious_force_push_bounded (reference from C do.c:1541-1573)
    // -------------------------------------------------------------
    #[test]
    fn prop_mysterious_force_matches_c_reference(
        depth in 0usize..40,
        bottom in 1usize..40,
        mf in 0u32..50,
        al in 0usize..4,
        t in any::<u32>(),
        a in any::<u32>(),
        b in any::<u32>(),
    ) {
        let align = [Alignment::Lawful, Alignment::Neutral, Alignment::Chaotic, Alignment::Unaligned][al];
        let got = mysterious_force(depth, bottom, mf, align, t, a, b);
        // C: active iff dunlev < dunlevs - 3; fires iff !rn2(4 + mf)
        let expected = if depth + 3 >= bottom || t % (4 + mf) != 0 {
            MysteriousForceOutcome::NoEffect
        } else {
            let odds: i64 = match align {
                Alignment::Lawful => 4,
                Alignment::Neutral => 3,
                Alignment::Chaotic => 2,
                Alignment::Unaligned => 3 - 128,
            };
            let mut diff = if odds <= 1 { 0 } else { (a as i64) % odds };
            if diff != 0 {
                let dest = (depth as i64 + (b as i64) % diff + 1).min(bottom as i64);
                diff = dest - depth as i64;
            }
            if diff == 0 {
                MysteriousForceOutcome::SameLevelTeleport
            } else {
                MysteriousForceOutcome::PushDown(depth + diff as usize)
            }
        };
        prop_assert_eq!(got, expected);
        // Bounds: push <= 3 lawful / 2 neutral / 1 chaotic, never in the bottom 4 levels.
        if let MysteriousForceOutcome::PushDown(d) = got {
            let cap = match align {
                Alignment::Lawful => 3,
                Alignment::Neutral => 2,
                _ => 1,
            };
            prop_assert!(d > depth && d - depth <= cap);
            prop_assert!(depth + 3 < bottom);
        }
        if depth + 3 >= bottom {
            prop_assert_eq!(got, MysteriousForceOutcome::NoEffect);
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
    // Theorem: poly_reversion_preserves_base_hp (C hack.c:4256 losehp /
    // polyself.c:1367 rehumanize; reference written from the C rule)
    // -------------------------------------------------------------
    #[test]
    fn prop_poly_damage_matches_c_reference(
        base_hp in 1i32..100,
        poly_hp in 1i32..100,
        damage in -20i32..250,
        polymorphed in any::<bool>(),
        unchanging in any::<bool>(),
    ) {
        use netrust_types::{Hero, PolymorphForm};
        use netrust_core::polymorph::{apply_poly_damage, PolyDamageResult};

        let mut hero = Hero { mount: None, quivered_item: None,
            base_hp,
            base_max_hp: base_hp,
            polymorph: polymorphed.then_some(PolymorphForm {
                monster_id: 1,
                hp: poly_hp,
                max_hp: poly_hp,
                duration: 100,
            }),
            lycanthropy: None, afflictions: Default::default(), skills: Default::default(),
        };

        // Reference: (result, base_hp after, poly hp after)
        let n = damage.max(0);
        let (exp, exp_base, exp_poly) = if polymorphed {
            let mh = poly_hp - n;
            if mh >= 1 {
                (PolyDamageResult::Absorbed, base_hp, Some(mh))
            } else if unchanging {
                (PolyDamageResult::Dead, base_hp, Some(mh))
            } else {
                (PolyDamageResult::Reverted, base_hp, None)
            }
        } else {
            let uhp = base_hp - n;
            if uhp < 1 {
                (PolyDamageResult::Dead, uhp, None)
            } else {
                (PolyDamageResult::BaseDamaged, uhp, None)
            }
        };

        let result = apply_poly_damage(&mut hero, damage, unchanging);
        prop_assert_eq!(result, exp);
        prop_assert_eq!(hero.base_hp, exp_base);
        prop_assert_eq!(hero.base_max_hp, base_hp);
        prop_assert_eq!(hero.polymorph.map(|p| p.hp), exp_poly);
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

/// C shk.c:2877-2988 `get_cost` (reference transcription for the proptest).
fn c_get_cost(base: u32, cha: i32, dunce: bool, unid: bool, artifact: bool, angry: bool) -> u32 {
    let mut tmp: i64 = if base == 0 { 5 } else { base as i64 };
    let (mut multiplier, mut divisor) = (1i64, 1i64);
    if unid {
        multiplier *= 4;
        divisor *= 3;
    }
    if dunce {
        multiplier *= 4;
        divisor *= 3;
    }
    if cha > 18 {
        divisor *= 2;
    } else if cha == 18 {
        multiplier *= 2;
        divisor *= 3;
    } else if cha >= 16 {
        multiplier *= 3;
        divisor *= 4;
    } else if cha <= 5 {
        multiplier *= 2;
    } else if cha <= 7 {
        multiplier *= 3;
        divisor *= 2;
    } else if cha <= 10 {
        multiplier *= 4;
        divisor *= 3;
    }
    tmp *= multiplier;
    if divisor > 1 {
        tmp *= 10;
        tmp /= divisor;
        tmp += 5;
        tmp /= 10;
    }
    if tmp <= 0 {
        tmp = 1;
    }
    if artifact {
        tmp *= 4;
    }
    if angry {
        tmp += (tmp + 2) / 3;
    }
    tmp as u32
}

/// C shk.c:3148-3192 `set_cost` for a non-gem stack (reference transcription).
fn c_set_cost(base: u32, dunce: bool, lowball: bool) -> u32 {
    let mut tmp = base as i64;
    let mut multiplier = 1i64;
    let mut divisor = if dunce { 3i64 } else { 2 };
    if lowball && tmp > 1 {
        multiplier *= 3;
        divisor *= 4;
    }
    if tmp >= 1 {
        tmp *= multiplier;
        if divisor > 1 {
            tmp *= 10;
            tmp /= divisor;
            tmp += 5;
            tmp /= 10;
        }
        if tmp < 1 {
            tmp = 1;
        }
    }
    tmp as u32
}

/// C priest.c:694-698: one iteration of the protection loop. `roll` is rn2(3)
/// when `ublessed == 0`, else rn2(ublessed).
fn c_ublessed_step(ublessed: u32, roll: u32) -> u32 {
    if ublessed == 0 {
        roll.min(2) + 2
    } else if ublessed < 20 && (ublessed < 9 || roll.min(ublessed - 1) == 0) {
        ublessed + 1
    } else {
        ublessed
    }
}

/// C priest.c:654-723: the donation band chosen for `offer` (gold already handed over).
fn c_priest_band(offer: u32, suggested: u32, quan: u32, gold_after: u32) -> DonationOutcome {
    let (offer, sq, gold) = (
        offer as i64,
        suggested as i64 * quan as i64,
        gold_after as i64,
    );
    if offer == 0 {
        DonationOutcome::Refused
    } else if offer < sq {
        if gold > offer * 2 {
            DonationOutcome::Cheapskate
        } else {
            DonationOutcome::Thanks
        }
    } else if offer < sq * 2 {
        DonationOutcome::Clairvoyance
    } else if offer < sq * 3 {
        DonationOutcome::Protection
    } else {
        DonationOutcome::Selfless
    }
}

//! Integration tests for hero Armor Class (AC) calculated from worn armor data.
//!
//! NetHack 5.0 C reference:
//! - `ARM_BONUS(obj)` (`include/hack.h:1526-1528`): `a_ac + spe - min(erosion, a_ac)`.
//! - `find_ac(void)` (`src/do_wear.c:2473-2507`): hero base AC 10, subtract ARM_BONUS
//!   for each worn slot (suit, cloak, helm, boots, shield, gloves, shirt) and divine
//!   protection `u.ublessed`.

use netrust_arena::{ItemLocation, ItemRecord};
use netrust_core::ActionAst;
use netrust_data::{create_item_record, CharacterConfig, ItemKindId, RoleId};
use netrust_sim::SimulationWorld;
use netrust_types::{Buc, ItemClass};

#[test]
fn test_fresh_valkyrie_starts_naked_at_ac_10() {
    // In NetHack 5.0 C (u_init.c:160), Valkyrie starts with small shield +3, spear +1,
    // dagger +0 and food ration, but NO body armor. Since NetRust has no small shield
    // item kind, the Valkyrie starts with no worn armor items, giving base AC 10
    // (mons[PM_HUMAN].ac, do_wear.c:2475).
    let sim = SimulationWorld::new_with_character(
        42,
        CharacterConfig {
            role: RoleId::Valkyrie,
            ..CharacterConfig::default()
        },
    );

    let player = sim.arena.actors.get(sim.player_id).unwrap();
    assert_eq!(
        player.ac, 10,
        "Fresh Valkyrie without armor should start at AC 10"
    );

    let carried_armor_count = sim
        .arena
        .items_carried_by(sim.player_id)
        .into_iter()
        .filter_map(|id| sim.arena.items.get(id))
        .filter(|it| it.class == ItemClass::Armor)
        .count();
    assert_eq!(
        carried_armor_count, 0,
        "Valkyrie starts with no armor in inventory"
    );
}

#[test]
fn test_fresh_rogue_starts_with_leather_armor_at_ac_8() {
    // C u_init.c:136: Rogue starts with leather armor (a_ac = 2).
    // find_ac: 10 - 2 = 8.
    let sim = SimulationWorld::new_with_character(
        42,
        CharacterConfig {
            role: RoleId::Rogue,
            ..CharacterConfig::default()
        },
    );

    let player = sim.arena.actors.get(sim.player_id).unwrap();
    assert_eq!(
        player.ac, 8,
        "Fresh Rogue carrying leather armor should start at AC 8"
    );
}

#[test]
fn test_fresh_wizard_starts_with_cloak_at_ac_9() {
    // C u_init.c:169: Wizard starts with cloak of magic resistance (a_ac = 1).
    // find_ac: 10 - 1 = 9.
    let sim = SimulationWorld::new_with_character(
        42,
        CharacterConfig {
            role: RoleId::Wizard,
            ..CharacterConfig::default()
        },
    );

    let player = sim.arena.actors.get(sim.player_id).unwrap();
    assert_eq!(
        player.ac, 9,
        "Fresh Wizard carrying cloak should start at AC 9"
    );
}

#[test]
fn test_leather_armor_plus_zero_reduces_ac_to_8() {
    let mut sim = SimulationWorld::new_with_character(
        42,
        CharacterConfig {
            role: RoleId::Valkyrie,
            ..CharacterConfig::default()
        },
    );
    assert_eq!(sim.arena.actors.get(sim.player_id).unwrap().ac, 10);

    // Spawn leather armor in hero's inventory
    let _armor_id = sim.arena.spawn_item(create_item_record(
        ItemKindId::LeatherArmor,
        ItemLocation::CarriedBy(sim.player_id),
        Buc::Uncursed,
    ));
    sim.recompute_hero_ac();

    assert_eq!(
        sim.arena.actors.get(sim.player_id).unwrap().ac,
        8,
        "Leather armor (+0) has a_ac 2, reducing AC from 10 to 8"
    );
}

#[test]
fn test_small_shield_plus_two_with_leather_armor() {
    let mut sim = SimulationWorld::new_with_character(
        42,
        CharacterConfig {
            role: RoleId::Valkyrie,
            ..CharacterConfig::default()
        },
    );

    // Leather armor (+0, a_ac = 2): ARM_BONUS = 2
    let _armor_id = sim.arena.spawn_item(create_item_record(
        ItemKindId::LeatherArmor,
        ItemLocation::CarriedBy(sim.player_id),
        Buc::Uncursed,
    ));

    // +2 Small shield (a_ac = 1, spe = 2): ARM_BONUS = 1 + 2 = 3
    let shield_rec = ItemRecord {
        name: "small shield".into(),
        class: ItemClass::Armor,
        weight: 30,
        buc: Buc::Uncursed,
        is_container: false,
        is_bag_of_holding: false,
        enchantment: 2,
        erosion: 0,
        proofed: false,
        location: ItemLocation::CarriedBy(sim.player_id),
        corpse_race: None,
        corpse_age: 0,
        rot_threshold: 50,
        recharged: 0,
    };
    sim.arena.spawn_item(shield_rec);

    sim.recompute_hero_ac();

    // find_ac = 10 - 2 (suit) - 3 (shield) = 5
    assert_eq!(
        sim.arena.actors.get(sim.player_id).unwrap().ac,
        5,
        "10 - ARM_BONUS(leather armor) - ARM_BONUS(+2 small shield) = 10 - 2 - 3 = 5"
    );
}

#[test]
fn test_two_helmets_do_not_stack() {
    let mut sim = SimulationWorld::new_with_character(
        42,
        CharacterConfig {
            role: RoleId::Valkyrie,
            ..CharacterConfig::default()
        },
    );

    // First helmet (+0, a_ac = 1): ARM_BONUS = 1 -> AC becomes 9
    let helm1 = ItemRecord {
        name: "helmet".into(),
        class: ItemClass::Armor,
        weight: 30,
        buc: Buc::Uncursed,
        is_container: false,
        is_bag_of_holding: false,
        enchantment: 0,
        erosion: 0,
        proofed: false,
        location: ItemLocation::CarriedBy(sim.player_id),
        corpse_race: None,
        corpse_age: 0,
        rot_threshold: 50,
        recharged: 0,
    };
    sim.arena.spawn_item(helm1);
    sim.recompute_hero_ac();
    assert_eq!(sim.arena.actors.get(sim.player_id).unwrap().ac, 9);

    // Second helmet (+0, a_ac = 1) also in inventory: should NOT stack!
    let helm2 = ItemRecord {
        name: "helmet".into(),
        class: ItemClass::Armor,
        weight: 30,
        buc: Buc::Uncursed,
        is_container: false,
        is_bag_of_holding: false,
        enchantment: 0,
        erosion: 0,
        proofed: false,
        location: ItemLocation::CarriedBy(sim.player_id),
        corpse_race: None,
        corpse_age: 0,
        rot_threshold: 50,
        recharged: 0,
    };
    sim.arena.spawn_item(helm2);
    sim.recompute_hero_ac();

    assert_eq!(
        sim.arena.actors.get(sim.player_id).unwrap().ac,
        9,
        "Carrying two helmets must not stack; only the first helmet per slot counts"
    );
}

#[test]
fn test_erosion_capping_in_arm_bonus() {
    let mut sim = SimulationWorld::new_with_character(
        42,
        CharacterConfig {
            role: RoleId::Valkyrie,
            ..CharacterConfig::default()
        },
    );

    // Leather armor with heavy erosion (erosion = 5, but a_ac = 2):
    // min(erosion, a_ac) = 2, so ARM_BONUS = 2 + 0 - 2 = 0
    let mut armor = create_item_record(
        ItemKindId::LeatherArmor,
        ItemLocation::CarriedBy(sim.player_id),
        Buc::Uncursed,
    );
    armor.erosion = 5;
    sim.arena.spawn_item(armor);
    sim.recompute_hero_ac();

    assert_eq!(
        sim.arena.actors.get(sim.player_id).unwrap().ac,
        10,
        "Heavy erosion on +0 leather armor reduces bonus to 0, leaving AC at 10"
    );

    // With spe +1, bonus is 2 + 1 - 2 = 1, AC becomes 9
    let mut enchanted_eroded_armor = create_item_record(
        ItemKindId::ChainMail,
        ItemLocation::CarriedBy(sim.player_id),
        Buc::Uncursed,
    );
    // Drop the leather armor so chain mail is the only suit
    let carried = sim.arena.items_carried_by(sim.player_id);
    for id in carried {
        if sim
            .arena
            .items
            .get(id)
            .map(|i| i.class == ItemClass::Armor)
            .unwrap_or(false)
        {
            sim.arena.destroy_item(id);
        }
    }
    enchanted_eroded_armor.enchantment = 1;
    enchanted_eroded_armor.erosion = 10; // C capped at a_ac (5)
    sim.arena.spawn_item(enchanted_eroded_armor);
    sim.recompute_hero_ac();

    // Chain mail a_ac = 5, spe = 1, min(10, 5) = 5 -> bonus = 5 + 1 - 5 = 1 -> AC 10 - 1 = 9
    assert_eq!(
        sim.arena.actors.get(sim.player_id).unwrap().ac,
        9,
        "Erosion cannot reduce bonus below enchantment spe"
    );
}

#[test]
fn test_divine_protection_lowers_hero_ac() {
    let mut sim = SimulationWorld::new_with_character(
        42,
        CharacterConfig {
            role: RoleId::Valkyrie,
            ..CharacterConfig::default()
        },
    );
    assert_eq!(sim.arena.actors.get(sim.player_id).unwrap().ac, 10);

    sim.divine_protection = 3;
    sim.recompute_hero_ac();

    assert_eq!(
        sim.arena.actors.get(sim.player_id).unwrap().ac,
        7,
        "Divine protection of 3 lowers hero AC by 3 (from 10 to 7)"
    );
}

#[test]
fn test_dropping_armor_restores_ac() {
    let mut sim = SimulationWorld::new_with_character(
        42,
        CharacterConfig {
            role: RoleId::Rogue,
            ..CharacterConfig::default()
        },
    );
    // Rogue starts with leather armor carried -> AC 8
    assert_eq!(sim.arena.actors.get(sim.player_id).unwrap().ac, 8);

    // Find the index of the leather armor in carried inventory
    let carried = sim.arena.items_carried_by(sim.player_id);
    let armor_idx = carried
        .iter()
        .position(|&id| sim.arena.items.get(id).unwrap().class == ItemClass::Armor)
        .unwrap();

    // Drop the leather armor
    sim.step_player_action(ActionAst::Drop(armor_idx));

    // AC should be back to base 10
    assert_eq!(
        sim.arena.actors.get(sim.player_id).unwrap().ac,
        10,
        "Dropping worn leather armor must restore base AC to 10"
    );
}

#[test]
fn test_container_transfer_updates_ac() {
    let mut sim = SimulationWorld::new_with_character(
        42,
        CharacterConfig {
            role: RoleId::Rogue,
            ..CharacterConfig::default()
        },
    );
    // Rogue has sack and leather armor -> AC 8
    assert_eq!(sim.arena.actors.get(sim.player_id).unwrap().ac, 8);

    let carried = sim.arena.items_carried_by(sim.player_id);
    let sack_idx = carried
        .iter()
        .position(|&id| sim.arena.items.get(id).unwrap().is_container)
        .unwrap();
    let armor_idx = carried
        .iter()
        .position(|&id| sim.arena.items.get(id).unwrap().class == ItemClass::Armor)
        .unwrap();

    // Put leather armor into sack
    sim.step_player_action(ActionAst::PutInContainer {
        item_index: armor_idx,
        container_index: sack_idx,
    });

    // Armor is now InContainer, not CarriedBy, so it is no longer worn
    assert_eq!(
        sim.arena.actors.get(sim.player_id).unwrap().ac,
        10,
        "Armor inside container is not worn; AC should be 10"
    );

    // Take leather armor back out of sack
    let carried_after = sim.arena.items_carried_by(sim.player_id);
    let sack_idx_after = carried_after
        .iter()
        .position(|&id| sim.arena.items.get(id).unwrap().is_container)
        .unwrap();

    sim.step_player_action(ActionAst::TakeFromContainer {
        container_index: sack_idx_after,
        item_index: 0,
    });

    // Armor is back in carried inventory -> AC 8
    assert_eq!(
        sim.arena.actors.get(sim.player_id).unwrap().ac,
        8,
        "Taking armor out of container into inventory makes it worn; AC should be 8"
    );
}

use netrust_data::ruleset::RulesetRef;
use netrust_pack::build;
use netrust_sim::{Alignment, CharacterConfig, Gender, RaceId, RoleId, SimulationWorld};
use std::path::PathBuf;
use std::sync::Arc;

#[test]
fn test_hard_mode_example_pack() {
    let pack_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("packs/examples/hard-mode");

    let (nrpack, report) = build(&pack_dir).expect("example pack should build successfully");
    assert!(
        report.is_ok(),
        "expected 0 errors, got: {:?}",
        report.errors
    );
    assert_eq!(
        report.warnings.len(),
        0,
        "expected 0 warnings, got: {:?}",
        report.warnings
    );

    let mut ruleset = nrpack.ruleset;
    ruleset.reindex();
    let ruleset_ref = RulesetRef {
        id: nrpack.manifest.id,
        version: nrpack.manifest.version,
        hash: nrpack.hash,
    };
    let ruleset_arc = Arc::new(ruleset);

    // Verify "dire jackal" exists and has level 2 and AC 6
    let dire_jackal = ruleset_arc
        .monster("dire jackal")
        .expect("dire jackal should exist in ruleset");
    assert_eq!(dire_jackal.level, 2);
    assert_eq!(dire_jackal.ac, 6);
    assert_eq!(dire_jackal.speed, 14);

    // Verify leather armor cost changed to 2
    let leather_armor = ruleset_arc
        .item("leather armor")
        .expect("leather armor should exist in ruleset");
    assert_eq!(leather_armor.cost, 2);

    // Test simulation world with Valkyrie character
    let config = CharacterConfig {
        name: "TestValkyrie".to_string(),
        role: RoleId::Valkyrie,
        race: RaceId::Human,
        gender: Gender::Female,
        alignment: Alignment::Neutral,
    };

    let world =
        SimulationWorld::new_with_character_and_ruleset(12345, config, ruleset_arc, ruleset_ref);

    // Check inventory for "food ration"
    let carried = world.arena.items_carried_by(world.player_id);
    let has_food_ration = carried.iter().any(|&item_id| {
        world
            .arena
            .items
            .get(item_id)
            .map(|item| item.name == "food ration")
            .unwrap_or(false)
    });
    assert!(
        has_food_ration,
        "Valkyrie should start with 'food ration' in inventory"
    );
}

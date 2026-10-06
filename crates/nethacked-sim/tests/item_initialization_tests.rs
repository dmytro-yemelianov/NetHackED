use nethacked_sim::SimulationWorld;
use nethacked_types::{BranchId, Buc, ItemClass};
use rand::RngCore;

#[test]
fn ordinary_rooms_initialize_objects_and_save_replays_same_rng() {
    let mut sampled_buc = 0;
    let mut sampled_stacks = 0;
    let mut sampled_spe = 0;
    for seed in 0..32 {
        let mut world = SimulationWorld::new_with_seed(seed);
        let save = world.to_save_json().unwrap();
        let mut loaded =
            SimulationWorld::from_save_json(&save, world.ruleset.clone(), &world.ruleset_ref)
                .unwrap();
        world.unpack_or_generate_level(BranchId::DungeonsOfDoom, 4);
        loaded.unpack_or_generate_level(BranchId::DungeonsOfDoom, 4);
        let items: Vec<_> = world.arena.items.iter().collect();
        let replayed: Vec<_> = loaded.arena.items.iter().collect();
        assert_eq!(items, replayed, "seed {seed}");
        assert_eq!(world.rng.next_u64(), loaded.rng.next_u64());
        for (_, item) in items {
            if !matches!(item.location, nethacked_arena::ItemLocation::Floor(_)) {
                continue;
            }
            sampled_buc += usize::from(item.buc != Buc::Uncursed);
            sampled_stacks += usize::from(item.quantity > 1);
            sampled_spe += usize::from(
                matches!(
                    item.class,
                    ItemClass::Weapon | ItemClass::Armor | ItemClass::Ring
                ) && item.enchantment != 0,
            );
            let encoded = serde_json::to_string(item).unwrap();
            assert_eq!(
                &serde_json::from_str::<nethacked_arena::ItemRecord>(&encoded).unwrap(),
                item
            );
        }
    }
    assert!(sampled_buc > 0);
    assert!(sampled_stacks > 0);
    assert!(sampled_spe > 0);
}

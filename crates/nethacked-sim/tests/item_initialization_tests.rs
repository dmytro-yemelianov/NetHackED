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

#[test]
fn ordinary_room_containers_attach_contents_and_roundtrip_rng() {
    use nethacked_arena::ItemLocation;
    let mut populated = 0;
    let mut locked = 0;
    for seed in 0..384 {
        let mut world = SimulationWorld::new_with_seed(seed);
        world.unpack_or_generate_level(BranchId::DungeonsOfDoom, 4);
        for (id, item) in world.arena.items.iter() {
            if let ItemLocation::InContainer(parent) = item.location {
                assert!(world.arena.items[parent].is_container);
                assert!(world.arena.items_in_container(parent).contains(&id));
                assert!(
                    world.arena.calculate_total_weight(parent) >= world.arena.items[parent].weight
                );
                populated += 1;
            }
            locked += usize::from(item.locked);
        }
        let save = world.to_save_json().unwrap();
        let mut loaded =
            SimulationWorld::from_save_json(&save, world.ruleset.clone(), &world.ruleset_ref)
                .unwrap();
        assert_eq!(
            world.arena.items.iter().collect::<Vec<_>>(),
            loaded.arena.items.iter().collect::<Vec<_>>()
        );
        assert_eq!(world.rng.next_u64(), loaded.rng.next_u64());
        world.unpack_or_generate_level(BranchId::DungeonsOfDoom, 5);
        loaded.unpack_or_generate_level(BranchId::DungeonsOfDoom, 5);
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&world.to_save_json().unwrap()).unwrap(),
            serde_json::from_str::<serde_json::Value>(&loaded.to_save_json().unwrap()).unwrap()
        );
    }
    assert!(
        populated > 0,
        "real ordinary rooms must generate contained objects"
    );
    assert!(locked > 0, "real room containers must sample lock state");
}

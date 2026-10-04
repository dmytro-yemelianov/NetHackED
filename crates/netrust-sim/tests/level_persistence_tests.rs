//! Level pack/unpack keeps every cross-entity reference valid.

use netrust_arena::ItemLocation;
use netrust_data::{create_item_record, create_monster_record, ItemKindId, MonsterSpeciesId};
use netrust_sim::{ActionAst, Buc, SimulationWorld};
use netrust_types::MountState;

fn go_down_and_up(sim: &mut SimulationWorld) {
    let down = sim.level.stairs_down;
    sim.arena.actors.get_mut(sim.player_id).unwrap().coord = down;
    sim.step_player_action(ActionAst::Descend);
    assert_eq!(sim.depth, 2);
    sim.step_player_action(ActionAst::Ascend);
    assert_eq!(sim.depth, 1);
}

fn assert_no_dangling_refs(sim: &SimulationWorld) {
    for (id, it) in sim.arena.items.iter() {
        match it.location {
            ItemLocation::InContainer(c) => assert!(sim.arena.items.contains_key(c), "item {id:?} in missing container"),
            ItemLocation::CarriedBy(a) => assert!(sim.arena.actors.contains_key(a), "item {id:?} carried by missing actor"),
            _ => {}
        }
    }
    for (iid, _) in &sim.unpaid_items {
        assert!(sim.arena.items.contains_key(*iid), "unpaid ledger references missing item");
    }
}

fn find_named(sim: &SimulationWorld, name: &str) -> Vec<netrust_arena::ItemId> {
    sim.arena.items.iter().filter(|(_, it)| it.name == name).map(|(id, _)| id).collect()
}

#[test]
fn nested_floor_containers_survive_round_trip() {
    let mut sim = SimulationWorld::new_with_seed(321);
    let spot = sim.level.stairs_down;
    let outer = sim.arena.spawn_item(create_item_record(ItemKindId::Sack, ItemLocation::Floor(spot), Buc::Uncursed));
    let mut inner_rec = create_item_record(ItemKindId::Sack, ItemLocation::InContainer(outer), Buc::Uncursed);
    inner_rec.name = "inner sack".into();
    let inner = sim.arena.spawn_item(inner_rec);
    let mut gem = create_item_record(ItemKindId::Dagger, ItemLocation::InContainer(inner), Buc::Uncursed);
    gem.name = "precious dagger".into();
    sim.arena.spawn_item(gem);

    go_down_and_up(&mut sim);

    assert_no_dangling_refs(&sim);
    let inner_ids = find_named(&sim, "inner sack");
    assert_eq!(inner_ids.len(), 1);
    let dagger_ids = find_named(&sim, "precious dagger");
    assert_eq!(dagger_ids.len(), 1);
    assert_eq!(sim.arena.items.get(dagger_ids[0]).unwrap().location, ItemLocation::InContainer(inner_ids[0]));
    match sim.arena.items.get(inner_ids[0]).unwrap().location {
        ItemLocation::InContainer(o) => assert!(matches!(sim.arena.items.get(o).unwrap().location, ItemLocation::Floor(_))),
        ref other => panic!("inner sack not in outer sack: {other:?}"),
    }
}

#[test]
fn hero_container_contents_untouched() {
    let mut sim = SimulationWorld::new_with_seed(322);
    let bag = sim.arena.spawn_item(create_item_record(ItemKindId::Sack, ItemLocation::CarriedBy(sim.player_id), Buc::Uncursed));
    let inside = sim.arena.spawn_item(create_item_record(ItemKindId::Dagger, ItemLocation::InContainer(bag), Buc::Uncursed));
    go_down_and_up(&mut sim);
    assert_eq!(sim.arena.items.get(inside).unwrap().location, ItemLocation::InContainer(bag));
    assert_no_dangling_refs(&sim);
}

#[test]
fn monster_inventory_survives_round_trip() {
    let mut sim = SimulationWorld::new_with_seed(323);
    let room = sim.level.rooms[0].center();
    let mut gob = create_monster_record(MonsterSpeciesId::Goblin, room);
    gob.name = "hoarder goblin".into();
    gob.speed = 0;
    let gid = sim.arena.spawn_actor(gob);
    let mut loot = create_item_record(ItemKindId::LongSword, ItemLocation::CarriedBy(gid), Buc::Uncursed);
    loot.name = "goblin loot".into();
    sim.arena.spawn_item(loot);

    go_down_and_up(&mut sim);

    assert_no_dangling_refs(&sim);
    let loot_id = find_named(&sim, "goblin loot")[0];
    let ItemLocation::CarriedBy(owner) = sim.arena.items.get(loot_id).unwrap().location else { panic!("loot not carried") };
    assert_eq!(sim.arena.actors.get(owner).unwrap().name, "hoarder goblin");
}

#[test]
fn unpaid_ledger_follows_items() {
    let mut sim = SimulationWorld::new_with_seed(324);
    if sim.unpaid_items.len() < 2 {
        return; // seed without a shop; covered by other seeds in CI sweep below
    }
    let (carried_id, _) = sim.unpaid_items[0];
    sim.arena.items.get_mut(carried_id).unwrap().location = ItemLocation::CarriedBy(sim.player_id);
    let floor_count = sim.unpaid_items.len() - 1;

    let down = sim.level.stairs_down;
    sim.arena.actors.get_mut(sim.player_id).unwrap().coord = down;
    sim.step_player_action(ActionAst::Descend);
    assert!(sim.is_unpaid(carried_id), "carried unpaid item lost its debt");
    sim.step_player_action(ActionAst::Ascend);

    assert!(sim.is_unpaid(carried_id));
    assert_eq!(sim.unpaid_items.len(), floor_count + 1);
    assert_no_dangling_refs(&sim);
}

#[test]
fn unpaid_ledger_follows_items_over_seeds() {
    let mut checked = 0;
    for seed in 0..30u64 {
        let mut sim = SimulationWorld::new_with_seed(seed);
        let before = sim.unpaid_items.len();
        if before == 0 {
            continue;
        }
        go_down_and_up(&mut sim);
        assert_eq!(sim.unpaid_items.len(), before, "seed {seed}");
        assert_no_dangling_refs(&sim);
        checked += 1;
    }
    assert!(checked > 0, "no seed produced a shop");
}

#[test]
fn steed_travels_with_hero() {
    let mut sim = SimulationWorld::new_with_seed(325);
    let p = sim.arena.actors.get(sim.player_id).unwrap().coord;
    let mut pony = create_monster_record(MonsterSpeciesId::Dog, p);
    pony.is_tame = true;
    let steed = sim.arena.spawn_actor(pony);
    sim.hero.mount = Some(MountState { steed_id: steed, saddle_equipped: true });

    let down = sim.level.stairs_down;
    sim.arena.actors.get_mut(sim.player_id).unwrap().coord = down;
    sim.step_player_action(ActionAst::Descend);

    assert!(sim.arena.actors.contains_key(steed));
    assert_eq!(sim.arena.actors.get(steed).unwrap().coord, sim.arena.actors.get(sim.player_id).unwrap().coord);
}

#[test]
fn quiver_cleared_when_item_left_behind() {
    let mut sim = SimulationWorld::new_with_seed(326);
    let spot = sim.level.stairs_down;
    let arrow = sim.arena.spawn_item(create_item_record(ItemKindId::Dagger, ItemLocation::Floor(spot), Buc::Uncursed));
    sim.hero.quivered_item = Some(arrow);
    sim.arena.actors.get_mut(sim.player_id).unwrap().coord = spot;
    sim.step_player_action(ActionAst::Descend);
    assert_eq!(sim.hero.quivered_item, None);
}

#[test]
fn container_cycle_does_not_hang_pack() {
    let mut sim = SimulationWorld::new_with_seed(322);
    let spot = sim.level.stairs_down;
    let floor = sim.arena.spawn_item(create_item_record(ItemKindId::Sack, ItemLocation::Floor(spot), Buc::Uncursed));
    let a = sim.arena.spawn_item(create_item_record(ItemKindId::Sack, ItemLocation::InContainer(floor), Buc::Uncursed));
    let b = sim.arena.spawn_item(create_item_record(ItemKindId::Sack, ItemLocation::InContainer(a), Buc::Uncursed));
    sim.arena.items.get_mut(a).unwrap().location = ItemLocation::InContainer(b);
    go_down_and_up(&mut sim);
    assert_no_dangling_refs(&sim);
}

#[test]
fn stale_wielded_item_cleared_on_descend() {
    let mut sim = SimulationWorld::new_with_seed(323);
    let spot = sim.level.stairs_down;
    let d = sim.arena.spawn_item(create_item_record(ItemKindId::Dagger, ItemLocation::Floor(spot), Buc::Uncursed));
    sim.wielded_item = Some(d);
    sim.arena.actors.get_mut(sim.player_id).unwrap().coord = spot;
    sim.step_player_action(ActionAst::Descend);
    assert_eq!(sim.depth, 2);
    assert_eq!(sim.wielded_item, None);
}

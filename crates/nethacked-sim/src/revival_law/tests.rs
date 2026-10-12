use super::*;
use nethacked_arena::EntityArena;
use nethacked_data::{ItemKindId, MonsterSpeciesId};
use nethacked_types::Buc;
use rand::{RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;

const ORIGIN: Coord = Coord::new_unchecked(20, 10);

fn input<'a>(
    ruleset: &'a Ruleset,
    arena: &'a EntityArena,
    level: &'a DungeonLevel,
    genocide: &'a GenocideRegistry,
    player: ActorId,
) -> RevivalInput<'a> {
    RevivalInput {
        ruleset,
        items: &arena.items,
        actors: &arena.actors,
        level,
        genocide_registry: genocide,
        player,
        branch: BranchId::DungeonsOfDoom,
        depth: 10,
        hero_alignment: Alignment::Neutral,
        hero_race: RaceId::Human,
        alignment_record: 0,
        role_name: "Valkyrie",
    }
}

fn hero(ruleset: &Ruleset, arena: &mut EntityArena) -> ActorId {
    arena.spawn_actor(
        ruleset
            .monster_by_id(MonsterSpeciesId::HUMAN)
            .unwrap()
            .create_record_with_hp(Coord::new_unchecked(5, 5), 10, 10, 18),
    )
}

#[test]
fn unsupported_inputs_use_no_rng_and_preserve_domain_facts() {
    let ruleset = Ruleset::vanilla();
    let mut arena = EntityArena::new();
    let player = hero(&ruleset, &mut arena);
    let stale = arena.spawn_item(
        ruleset
            .create_corpse_record("troll", ItemLocation::Floor(ORIGIN))
            .unwrap(),
    );
    arena.items.remove(stale);
    let ordinary = arena.spawn_item(
        ruleset
            .create_item_record_by_id(ItemKindId::SACK, ItemLocation::Floor(ORIGIN), Buc::Uncursed)
            .unwrap(),
    );
    let nontroll = arena.spawn_item(
        ruleset
            .create_corpse_record("jackal", ItemLocation::Floor(ORIGIN))
            .unwrap(),
    );
    let level = DungeonLevel::new_solid(Tile::Room);
    let genocide = GenocideRegistry::default();
    let before = serde_json::to_value((&arena, &level, &genocide)).unwrap();
    let facts = input(&ruleset, &arena, &level, &genocide, player);
    for id in [stale, ordinary, nontroll] {
        let mut rng = ChaCha8Rng::seed_from_u64(61);
        let mut expected = rng.clone();
        assert!(matches!(
            ordinary_troll_attempt(id, &facts, &mut rng),
            RevivalAttempt::Unsupported
        ));
        assert_eq!(rng.next_u64(), expected.next_u64());
    }
    assert_eq!(
        serde_json::to_value((&arena, &level, &genocide)).unwrap(),
        before
    );
}

#[test]
fn revived_actor_is_returned_without_inserting_or_consuming_stack_suffix() {
    let ruleset = Ruleset::vanilla();
    let mut arena = EntityArena::new();
    let player = hero(&ruleset, &mut arena);
    let mut item = ruleset
        .create_corpse_record("troll", ItemLocation::Floor(ORIGIN))
        .unwrap();
    item.enchantment = 1;
    item.quantity = 3;
    let id = arena.spawn_item(item);
    let level = DungeonLevel::new_solid(Tile::Room);
    let genocide = GenocideRegistry::default();
    let before = serde_json::to_value((&arena, &level, &genocide)).unwrap();
    let mut rng = ChaCha8Rng::seed_from_u64(61);
    let facts = input(&ruleset, &arena, &level, &genocide, player);
    let RevivalAttempt::Revived { actor } = ordinary_troll_attempt(id, &facts, &mut rng) else {
        panic!("ordinary floor Troll should return an actor");
    };
    assert_eq!(
        (actor.coord, actor.level, actor.hp, actor.max_hp),
        (ORIGIN, 9, 41, 41)
    );
    assert_eq!(actor.sex, Some(ActorSex::Female));
    let mut expected = ChaCha8Rng::seed_from_u64(61);
    let _identity = expected.random_range(0..2u32);
    for _ in 0..9 {
        let _hp = expected.random_range(0..8u32);
    }
    // The actual insertion and its subsequent split draw belong to the adapter.
    for _ in 0..4 {
        assert_eq!(rng.next_u64(), expected.next_u64());
    }
    assert_eq!(arena.actors.len(), 1);
    assert_eq!(arena.items[id].quantity, 3);
    assert_eq!(
        serde_json::to_value((&arena, &level, &genocide)).unwrap(),
        before
    );
}

#[test]
fn failed_attempt_returns_clamped_delay_without_applying_owner_effects() {
    let ruleset = Ruleset::vanilla();
    let mut arena = EntityArena::new();
    let player = hero(&ruleset, &mut arena);
    let mut item = ruleset
        .create_corpse_record("troll", ItemLocation::Floor(ORIGIN))
        .unwrap();
    item.corpse_no_revive = true;
    item.corpse_age = 300;
    let id = arena.spawn_item(item);
    let level = DungeonLevel::new_solid(Tile::Room);
    let genocide = GenocideRegistry::default();
    let before = serde_json::to_value((&arena, &level, &genocide)).unwrap();
    let facts = input(&ruleset, &arena, &level, &genocide, player);
    let mut rng = ChaCha8Rng::seed_from_u64(61);
    assert!(matches!(
        ordinary_troll_attempt(id, &facts, &mut rng),
        RevivalAttempt::Failed { rot_delay: 1 }
    ));
    let mut expected = ChaCha8Rng::seed_from_u64(61);
    for _ in 0..5 {
        let _rot = expected.random_range(0..50u32);
    }
    for _ in 0..4 {
        assert_eq!(rng.next_u64(), expected.next_u64());
    }
    assert_eq!(
        serde_json::to_value((&arena, &level, &genocide)).unwrap(),
        before
    );
}

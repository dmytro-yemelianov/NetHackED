use nethacked_arena::{ActorRecord, ItemLocation};
use nethacked_data::containers::{ContainerContext, GeneratedItem};
use nethacked_data::generation::{DungeonAlign, MonsterGenContext};
use nethacked_data::monsters::{CorpseTimeoutPolicy, MonsterDraw, MonsterSpawnContext};
use nethacked_data::{ItemKindId, MonsterDef, MonsterSpeciesId as M, Ruleset};
use nethacked_types::{Coord, EndgamePlane};
use rand::{rngs::StdRng, Rng, RngCore, SeedableRng};

#[derive(Debug, PartialEq, Eq)]
struct Refused(usize);

fn coord() -> Coord {
    Coord::new_unchecked(3, 4)
}

fn assert_actor_equal(a: &ActorRecord, b: &ActorRecord) {
    assert_eq!(
        serde_json::to_value(a).unwrap(),
        serde_json::to_value(b).unwrap()
    );
}

fn assert_item_equal(a: Option<GeneratedItem>, b: Option<GeneratedItem>) {
    match (a, b) {
        (Some(a), Some(b)) => {
            assert_eq!(
                serde_json::to_value(a.item).unwrap(),
                serde_json::to_value(b.item).unwrap()
            );
            assert!(a.contents.is_empty() && b.contents.is_empty());
            assert_eq!(a.corpse_timeout, b.corpse_timeout);
            assert_eq!(a.corpse_timer_operations, b.corpse_timer_operations);
        }
        (None, None) => {}
        _ => panic!("different Option behavior"),
    }
}

fn prove_monster(rules: &Ruleset, def: &MonsterDef, context: MonsterSpawnContext, seed: u64) {
    let mut legacy_rng = StdRng::seed_from_u64(seed);
    let mut try_rng = legacy_rng.clone();
    let mut trace = Vec::new();
    let legacy = rules.create_random_monster_record_from_def(
        def,
        coord(),
        context,
        (10, 18),
        &mut legacy_rng,
    );
    let actual = rules
        .try_create_random_monster_record_from_def(def, coord(), context, (10, 18), |draw| {
            let value = draw.sample(&mut try_rng);
            trace.push((draw, value));
            Ok::<_, Refused>(value)
        })
        .unwrap();
    assert_actor_equal(&legacy, &actual);
    assert_eq!(legacy_rng.next_u64(), try_rng.next_u64());
    assert_eq!(trace[0].0, MonsterDraw::Below(2));
    for fail_at in 0..trace.len() {
        let mut rng = StdRng::seed_from_u64(seed);
        let mut expected_rng = rng.clone();
        for &(draw, value) in &trace[..fail_at] {
            assert_eq!(draw.sample(&mut expected_rng), value);
        }
        let mut calls = 0;
        let result = rules.try_create_random_monster_record_from_def(
            def,
            coord(),
            context,
            (10, 18),
            |draw| {
                let index = calls;
                calls += 1;
                assert_eq!(draw, trace[index].0);
                if index == fail_at {
                    return Err(Refused(index));
                }
                let value = draw.sample(&mut rng);
                assert_eq!(value, trace[index].1);
                Ok(value)
            },
        );
        assert!(matches!(result, Err(Refused(index)) if index == fail_at));
        assert_eq!(calls, fail_at + 1);
        assert_eq!(rng.next_u64(), expected_rng.next_u64());
    }
}

#[test]
fn canonical_and_custom_monster_outputs_continuation_and_every_refusal() {
    let rules = Ruleset::vanilla();
    for seed in 0..8 {
        for id in [
            M::JACKAL,
            M::WARG,
            M::DEATH,
            M::RED_DRAGON,
            M::IRON_GOLEM,
            M::AIR_ELEMENTAL,
        ] {
            for plane in [None, Some(EndgamePlane::Air), Some(EndgamePlane::Astral)] {
                prove_monster(
                    &rules,
                    rules.monster_by_id(id).unwrap(),
                    MonsterSpawnContext {
                        endgame_plane: plane,
                    },
                    seed,
                );
            }
        }
        for level in [0, 1, 7, 50] {
            for fixed in [false, true] {
                for sex in [None, Some("male"), Some("female"), Some("neuter")] {
                    let mut def = rules.monster_by_id(M::JACKAL).unwrap().clone();
                    def.id = None;
                    def.name = "custom specimen".into();
                    def.level = level;
                    def.sample_hp = !fixed;
                    def.base_hp = 19;
                    def.max_hp = 23;
                    def.flags = sex.into_iter().map(str::to_owned).collect();
                    prove_monster(&rules, &def, MonsterSpawnContext::default(), seed);
                }
            }
        }
    }
}

fn contexts() -> (ContainerContext, MonsterGenContext) {
    (
        ContainerContext {
            hero_level: 18,
            level_difficulty: 10,
            moves: 73,
            in_mklev: false,
        },
        MonsterGenContext {
            hero_level: 18,
            level_difficulty: 10,
            in_hell: false,
            align: DungeonAlign::None,
            temperature: 0,
        },
    )
}

fn corpse_rules(initial: usize, final_kind: usize) -> Ruleset {
    let mut rules = (*Ruleset::vanilla()).clone();
    for def in &mut rules.monsters {
        def.frequency = u32::from(def.id == Some(M::JACKAL));
        def.difficulty = 5;
        def.gen_flags
            .retain(|f| !["nogen", "uniq", "hell", "nohell"].contains(&f.as_str()));
    }
    let def = rules
        .monsters
        .iter_mut()
        .find(|d| d.id == Some(M::JACKAL))
        .unwrap();
    def.flags.clear();
    def.glyph = if initial == 1 { 'T' } else { 'd' };
    def.corpse_timeout_policy = policy(initial);
    let mut final_def = def.clone();
    final_def.id = None;
    final_def.name = "final specimen".into();
    final_def.frequency = 0;
    final_def.flags = vec!["male".into()];
    final_def.glyph = if final_kind == 1 { 'T' } else { 'd' };
    final_def.corpse_timeout_policy = policy(final_kind);
    rules.monsters.push(final_def);
    rules.reindex();
    rules
}

fn policy(kind: usize) -> CorpseTimeoutPolicy {
    match kind {
        2 => CorpseTimeoutPolicy::Never,
        3 => CorpseTimeoutPolicy::Rider,
        _ => CorpseTimeoutPolicy::Ordinary,
    }
}

fn prove_corpse(rules: &Ruleset, actor: &ActorRecord, ctx: ContainerContext, seed: u64) -> usize {
    let (_, monsters) = contexts();
    let location = ItemLocation::Floor(coord());
    let mut legacy_rng = StdRng::seed_from_u64(seed);
    let mut try_rng = legacy_rng.clone();
    let mut trace = Vec::new();
    let legacy = rules.create_initialized_corpse_for_actor(
        actor,
        location.clone(),
        ctx,
        &monsters,
        |_| false,
        |n| legacy_rng.random_range(0..n),
    );
    let actual = rules
        .try_create_initialized_corpse_for_actor(
            actor,
            location.clone(),
            ctx,
            &monsters,
            |_| false,
            |n| {
                let value = try_rng.random_range(0..n);
                trace.push((n, value));
                Ok::<_, Refused>(value)
            },
        )
        .unwrap();
    assert_item_equal(legacy, actual);
    assert_eq!(legacy_rng.next_u64(), try_rng.next_u64());
    for fail_at in 0..trace.len() {
        let mut rng = StdRng::seed_from_u64(seed);
        let mut expected_rng = rng.clone();
        for &(n, value) in &trace[..fail_at] {
            assert_eq!(expected_rng.random_range(0..n), value);
        }
        let mut calls = 0;
        let result = rules.try_create_initialized_corpse_for_actor(
            actor,
            location.clone(),
            ctx,
            &monsters,
            |_| false,
            |n| {
                let index = calls;
                calls += 1;
                assert_eq!(n, trace[index].0);
                if index == fail_at {
                    return Err(Refused(index));
                }
                let value = rng.random_range(0..n);
                assert_eq!(value, trace[index].1);
                Ok(value)
            },
        );
        assert!(matches!(result, Err(Refused(index)) if index == fail_at));
        assert_eq!(calls, fail_at + 1);
        assert_eq!(rng.next_u64(), expected_rng.next_u64());
    }
    trace.len()
}

#[test]
fn nested_corpse_timeout_transitions_and_every_refusal() {
    for initial in 0..4 {
        for final_kind in 0..4 {
            let rules = corpse_rules(initial, final_kind);
            let mut actor = rules
                .monster("final specimen")
                .unwrap()
                .create_record_with_hp(coord(), 0, 7, 3);
            actor.is_tame = true;
            actor.is_cancelled = true;
            for seed in 0..4 {
                let (mut ctx, _) = contexts();
                ctx.in_mklev = seed % 2 == 0;
                assert!(prove_corpse(&rules, &actor, ctx, seed) >= 2);
            }
        }
    }
    let rules = Ruleset::vanilla();
    for id in [M::JACKAL, M::TROLL, M::DEATH, M::LIZARD, M::HUMAN_ZOMBIE] {
        let actor = rules
            .monster_by_id(id)
            .unwrap()
            .create_record_with_hp(coord(), 0, 7, 3);
        prove_corpse(&rules, &actor, contexts().0, 7);
    }
}

#[test]
fn corpse_fallback_and_no_entropy_cases() {
    let (ctx, monsters) = contexts();
    let mut rules = corpse_rules(0, 0);
    // No reservoir candidates: exercise canonical fallback rejection.
    for def in &mut rules.monsters {
        def.frequency = 0;
    }
    rules.reindex();
    let actor = rules
        .monster("final specimen")
        .unwrap()
        .create_record_with_hp(coord(), 0, 7, 3);
    assert!(prove_corpse(&rules, &actor, ctx, 2) > 0);
    // No eligible fallback: all 256 attempts fail, followed by no initial recipe.
    rules.monsters.retain(|d| d.name == "final specimen");
    rules.reindex();
    assert_eq!(prove_corpse(&rules, &actor, ctx, 2), 256);
    rules.monsters[0].gen_flags.push("nocorpse".into());
    rules.reindex();
    let result = rules.try_create_initialized_corpse_for_actor(
        &actor,
        ItemLocation::Floor(coord()),
        ctx,
        &monsters,
        |_| false,
        |_| -> Result<u32, Refused> { panic!("no-corpse draws nothing") },
    );
    assert_eq!(result, Ok(None));
    rules.monsters[0].gen_flags.clear();
    rules.items.retain(|d| d.id != Some(ItemKindId::CORPSE));
    rules.reindex();
    let result = rules.try_create_initialized_corpse_for_actor(
        &actor,
        ItemLocation::Floor(coord()),
        ctx,
        &monsters,
        |_| false,
        |_| -> Result<u32, Refused> { panic!("missing corpse definition draws nothing") },
    );
    assert_eq!(result, Ok(None));
}

#[test]
fn unresolved_actor_returns_unspecified_corpse_without_entropy() {
    let rules = Ruleset::vanilla();
    let mut actor = rules
        .monster_by_id(M::JACKAL)
        .unwrap()
        .create_record_with_hp(coord(), 0, 7, 3);
    actor.species = None;
    actor.name = "unresolved legacy actor".into();
    let (ctx, monsters) = contexts();
    let legacy = rules.create_initialized_corpse_for_actor(
        &actor,
        ItemLocation::Floor(coord()),
        ctx,
        &monsters,
        |_| false,
        |_| panic!("unresolved identity draws nothing"),
    );
    let actual = rules
        .try_create_initialized_corpse_for_actor(
            &actor,
            ItemLocation::Floor(coord()),
            ctx,
            &monsters,
            |_| false,
            |_| -> Result<u32, Refused> { panic!("unresolved identity draws nothing") },
        )
        .unwrap();
    assert!(actual.is_some());
    assert_item_equal(legacy, actual);
}

#[test]
fn every_refusal_in_full_troll_trial_sequences() {
    let rules = corpse_rules(1, 1);
    let actor = rules
        .monster("final specimen")
        .unwrap()
        .create_record_with_hp(coord(), 0, 7, 3);
    let (ctx, monsters) = contexts();
    let mut trace = Vec::new();
    let expected = rules.create_initialized_corpse_for_actor(
        &actor,
        ItemLocation::Floor(coord()),
        ctx,
        &monsters,
        |_| false,
        |n| {
            trace.push(n);
            n - 1
        },
    );
    assert_eq!(trace.iter().filter(|&&n| n == 37).count(), 98);
    let actual = rules
        .try_create_initialized_corpse_for_actor(
            &actor,
            ItemLocation::Floor(coord()),
            ctx,
            &monsters,
            |_| false,
            |n| Ok::<_, Refused>(n - 1),
        )
        .unwrap();
    assert_item_equal(expected, actual);
    for fail_at in 0..trace.len() {
        let mut calls = 0;
        let result = rules.try_create_initialized_corpse_for_actor(
            &actor,
            ItemLocation::Floor(coord()),
            ctx,
            &monsters,
            |_| false,
            |n| {
                let index = calls;
                calls += 1;
                assert_eq!(n, trace[index]);
                if index == fail_at {
                    Err(Refused(index))
                } else {
                    Ok(n - 1)
                }
            },
        );
        assert!(matches!(result, Err(Refused(index)) if index == fail_at));
        assert_eq!(calls, fail_at + 1);
    }
}

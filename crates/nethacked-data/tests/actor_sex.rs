//! Source-only expectations from NetHack 5.0 makemon.c:1258–1279.
use nethacked_arena::{ActorSex, EntityArena};
use nethacked_data::monsters::MonsterSpawnContext;
use nethacked_data::{CharacterConfig, Gender, MonsterSpeciesId as M, Ruleset};
use nethacked_types::Coord;
use rand::{RngCore, SeedableRng};

struct Words<'a>(std::iter::Copied<std::slice::Iter<'a, u32>>);
impl<'a> Words<'a> {
    fn new(words: &'a [u32]) -> Self {
        Self(words.iter().copied())
    }
    fn exhausted(&self) {
        assert_eq!(self.0.len(), 0, "skipped source draw");
    }
}
impl RngCore for Words<'_> {
    fn next_u32(&mut self) -> u32 {
        self.0.next().expect("extra source draw")
    }
    fn next_u64(&mut self) -> u64 {
        panic!("expected bounded u32 draw")
    }
    fn fill_bytes(&mut self, _: &mut [u8]) {
        panic!("expected bounded u32 draw")
    }
}
const COORD: Coord = Coord::new_unchecked(8, 8);

#[test]
fn fixed_flags_and_variable_literal_draws_use_owned_definition() {
    let rules = Ruleset::vanilla();
    let mut def = rules.monster("jackal").unwrap().clone();
    def.name = "Medusa".into(); // display/catalog identity cannot choose sex
    def.id = None;
    def.sample_hp = false;
    for (flags, expected) in [
        (vec!["male"], ActorSex::Male),
        (vec!["female"], ActorSex::Female),
        (vec!["neuter"], ActorSex::Neuter),
        (vec!["female", "male", "neuter"], ActorSex::Female),
        (vec!["male", "neuter"], ActorSex::Male),
    ] {
        def.flags = flags.into_iter().map(String::from).collect();
        let mut rng = Words::new(&[]);
        let actor = rules.create_monster_record_from_def(
            &def,
            COORD,
            MonsterSpawnContext::default(),
            &mut rng,
        );
        assert_eq!(actor.sex, Some(expected));
        assert_eq!(
            def.create_record_with_hp(COORD, 31, 41, 6).sex,
            Some(expected)
        );
        rng.exhausted();
    }
    def.flags.clear();
    assert_eq!(def.create_record_with_hp(COORD, 31, 41, 6).sex, None);
    for (word, expected) in [
        (0x4000_0000, ActorSex::Male),
        (0xc000_0000, ActorSex::Female),
    ] {
        let words = [word];
        let mut rng = Words::new(&words);
        assert_eq!(
            rules
                .create_monster_record_from_def(
                    &def,
                    COORD,
                    MonsterSpawnContext::default(),
                    &mut rng,
                )
                .sex,
            Some(expected)
        );
        rng.exhausted();
    }
}

#[test]
fn all_public_rng_factories_draw_sex_once_after_literal_hp_draws() {
    let mut rules = (*Ruleset::vanilla()).clone();
    let def = rules
        .monsters
        .iter_mut()
        .find(|d| d.id == Some(M::JACKAL))
        .unwrap();
    def.level = 2;
    def.flags.clear();
    rules.reindex();
    // d8 results 1 and 8 -> HP 9; rn2(2) result 1 -> female.
    // Sampling sex first would yield male and different HP.
    for route in 0..5 {
        let mut rng = Words::new(&[0x1000_0000, 0xf000_0000, 0xc000_0000]);
        let ctx = MonsterSpawnContext::default();
        let actor = match route {
            0 => rules
                .create_monster_record("jackal", COORD, &mut rng)
                .unwrap(),
            1 => rules
                .create_monster_record_with_context("jackal", COORD, ctx, &mut rng)
                .unwrap(),
            2 => rules
                .create_monster_record_by_id(M::JACKAL, COORD, &mut rng)
                .unwrap(),
            3 => rules
                .create_monster_record_by_id_with_context(M::JACKAL, COORD, ctx, &mut rng)
                .unwrap(),
            _ => rules.create_monster_record_from_def(
                rules.monster("jackal").unwrap(),
                COORD,
                ctx,
                &mut rng,
            ),
        };
        assert_eq!((actor.hp, actor.max_hp, actor.level), (9, 9, 2));
        assert_eq!(actor.sex, Some(ActorSex::Female));
        rng.exhausted();
    }
    // Free catalog wrapper follows the same sampler; vanilla level-zero jackal is one d4.
    let mut rng = Words::new(&[0xf000_0000, 0x4000_0000]);
    let actor = nethacked_data::create_monster_record(M::JACKAL, COORD, &mut rng);
    assert_eq!((actor.hp, actor.sex), (4, Some(ActorSex::Male)));
    rng.exhausted();
}

#[test]
fn renamed_builtin_owned_sex_overrides_catalog_and_pure_helpers_do_not_sample() {
    let mut rules = (*Ruleset::vanilla()).clone();
    let def = rules
        .monsters
        .iter_mut()
        .find(|d| d.id == Some(M::MEDUSA))
        .unwrap();
    def.name = "Owned serpent".into();
    def.flags
        .retain(|f| !matches!(f.as_str(), "female" | "male" | "neuter"));
    def.flags.push("male".into());
    def.sample_hp = false;
    rules.reindex();
    let mut rng = Words::new(&[]);
    assert_eq!(
        rules
            .create_monster_record_by_id(M::MEDUSA, COORD, &mut rng)
            .unwrap()
            .sex,
        Some(ActorSex::Male)
    );
    assert_eq!(
        rules
            .create_monster_record("Owned serpent", COORD, &mut rng)
            .unwrap()
            .sex,
        Some(ActorSex::Male)
    );
    rng.exhausted();
    assert_eq!(
        nethacked_data::create_ghost_record("donor", 4, 41, COORD).sex,
        None
    );
}

#[test]
fn player_factories_use_actual_config_without_sampling() {
    for (gender, sex) in [
        (Gender::Male, ActorSex::Male),
        (Gender::Female, ActorSex::Female),
    ] {
        let cfg = CharacterConfig {
            gender,
            ..CharacterConfig::default()
        };
        for owned in [false, true] {
            let mut arena = EntityArena::new();
            let (id, _) = if owned {
                Ruleset::vanilla().spawn_player_character(&cfg, COORD, &mut arena)
            } else {
                nethacked_data::spawn_player_character(&cfg, COORD, &mut arena)
            };
            assert_eq!(arena.actors[id].sex, Some(sex));
        }
    }
    let mut rng = rand::rngs::StdRng::seed_from_u64(6);
    let actor = nethacked_data::create_monster_record(M::JACKAL, COORD, &mut rng);
    let mut old = serde_json::to_value(actor).unwrap();
    old.as_object_mut().unwrap().remove("sex");
    let loaded: nethacked_arena::ActorRecord = serde_json::from_value(old.clone()).unwrap();
    assert_eq!(loaded.sex, None);
    assert_eq!(serde_json::to_value(&loaded).unwrap(), old);
    for sex in [ActorSex::Male, ActorSex::Female, ActorSex::Neuter] {
        let mut known = loaded.clone();
        known.sex = Some(sex);
        assert_eq!(
            serde_json::from_value::<nethacked_arena::ActorRecord>(
                serde_json::to_value(&known).unwrap()
            )
            .unwrap(),
            known
        );
    }
}

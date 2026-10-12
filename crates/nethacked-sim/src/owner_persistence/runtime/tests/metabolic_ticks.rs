use super::super::*;
use nethacked_types::Locale;
fn world(n: i32, turn: u64, hp: u32, dead: bool, locale: Locale) -> SimulationWorld {
    let mut w = SimulationWorld::new_with_seed(819);
    w.hero.skills.skills.clear();
    w.arena.actors.retain(|id, _| id == w.player_id);
    w.player_nutrition = n;
    w.scheduler.turn = turn;
    w.locale = locale;
    w.arena.actors[w.player_id].hp = hp;
    w.arena.actors[w.player_id].is_dead = dead;
    w.max_event_log_len = Some(2);
    w.record_events(&[crate::GameEvent::LogMessage { text: "old".into() }]);
    w
}
fn config(w: &SimulationWorld) -> OwnerBridgeConfig {
    OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone())
        .unwrap()
        .with_passive_metabolic_ticks()
}
fn bridge(w: SimulationWorld, c: OwnerBridgeConfig) -> OwnerBridge {
    OwnerBridge::install(NativePartitions::from_native(w), c).unwrap()
}
fn value(w: &SimulationWorld) -> serde_json::Value {
    serde_json::from_str(&w.to_save_json().unwrap()).unwrap()
}
// Independent literal original inline block, not the extracted law.
fn oracle(w: &mut SimulationWorld) -> Vec<crate::GameEvent> {
    use nethacked_core::HungerState;
    let mut events = vec![];
    let old = w.hunger_state();
    w.player_nutrition = nethacked_core::nutrition::metabolic_tick(w.player_nutrition);
    let new = w.hunger_state();
    if old != new && new == HungerState::Hungry {
        events.push(crate::GameEvent::LogMessage {
            text: nethacked_i18n::Messages::hunger_hungry(w.locale).into(),
        });
    } else if old != new && new == HungerState::Weak {
        events.push(crate::GameEvent::LogMessage {
            text: nethacked_i18n::Messages::hunger_weak(w.locale).into(),
        });
    } else if new == HungerState::Fainting && w.scheduler.turn % 10 == 0 {
        let mut died = false;
        if let Some(p) = w.arena.actors.get_mut(w.player_id) {
            if !p.is_dead {
                p.hp = p.hp.saturating_sub(1);
                if p.hp == 0 {
                    p.is_dead = true;
                    died = true;
                }
            }
        }
        events.push(crate::GameEvent::LogMessage {
            text: nethacked_i18n::Messages::hunger_fainting(w.locale).into(),
        });
        if died {
            events.push(crate::GameEvent::LogMessage {
                text: nethacked_i18n::t("fainted_death", w.locale).into(),
            });
        }
    } else if new == HungerState::Starved {
        if let Some(p) = w.arena.actors.get_mut(w.player_id) {
            if !p.is_dead {
                p.hp = 0;
                p.is_dead = true;
                events.push(crate::GameEvent::LogMessage {
                    text: nethacked_i18n::t("starved", w.locale).into(),
                });
            }
        }
    }
    w.record_events(&events);
    events
}
fn compare(w: SimulationWorld) {
    let c = config(&w);
    let b = bridge(w.clone(), c);
    let mut expected = w;
    let events = oracle(&mut expected);
    let out = b
        .execute_metabolic_phase(b.prepare_metabolic_phase().unwrap())
        .unwrap();
    assert!(out.error.is_none(), "{:?}", out.error);
    assert_eq!(out.events, events);
    assert!(value(&b.export_native().unwrap()) == value(&expected));
}
#[test]
fn literal_thresholds_locale_and_full_native_phase_equality() {
    for locale in [Locale::En, Locale::Uk] {
        for n in [
            1002,
            1001,
            1000,
            152,
            151,
            150,
            52,
            51,
            50,
            2,
            1,
            0,
            -199,
            -200,
            -201,
            i32::MIN,
        ] {
            compare(world(n, 9, 2, false, locale));
        }
    }
    // Literal known threshold witnesses and death message order, beyond oracle.
    let mut w = world(151, 9, 2, false, Locale::En);
    let mut events = vec![];
    w.tick_passive_metabolism(&mut events);
    assert_eq!(w.player_nutrition, 150);
    assert_eq!(
        events,
        vec![crate::GameEvent::LogMessage {
            text: "You are beginning to feel hungry.".into()
        }]
    );
    let w = world(0, 10, 1, false, Locale::En);
    let b = bridge(w.clone(), config(&w));
    let out = b
        .execute_metabolic_phase(b.prepare_metabolic_phase().unwrap())
        .unwrap();
    assert_eq!(
        out.events,
        vec![
            crate::GameEvent::LogMessage {
                text: nethacked_i18n::Messages::hunger_fainting(Locale::En).into()
            },
            crate::GameEvent::LogMessage {
                text: nethacked_i18n::t("fainted_death", Locale::En).into()
            }
        ]
    );
}
#[test]
fn fainting_turn_hp_dead_and_starvation_log_once() {
    for locale in [Locale::En, Locale::Uk] {
        for turn in [9, 10, 20] {
            for hp in [0, 1, 2] {
                for dead in [false, true] {
                    compare(world(1, turn, hp, dead, locale));
                }
            }
        }
    }
    for dead in [false, true] {
        let w = world(-200, 10, 2, dead, Locale::En);
        let b = bridge(w.clone(), config(&w));
        let mut expected = w;
        for _ in 0..2 {
            let events = oracle(&mut expected);
            let out = b
                .execute_metabolic_phase(b.prepare_metabolic_phase().unwrap())
                .unwrap();
            assert!(out.error.is_none());
            assert_eq!(out.events, events);
            assert!(value(&b.export_native().unwrap()) == value(&expected));
        }
        assert!(expected.arena.actors[expected.player_id].is_dead);
    }
}
#[test]
fn authentic_request_replay_completed_reload_and_historical_profiles() {
    let w = world(151, 9, 2, false, Locale::En);
    let c = config(&w);
    let b = bridge(w, c.clone());
    let req = b.prepare_metabolic_phase().unwrap();
    b.execute_metabolic_phase(req.clone()).unwrap();
    let after = value(&b.export_native().unwrap());
    assert!(b.execute_metabolic_phase(req).is_err());
    assert!(value(&b.export_native().unwrap()) == after);
    let saved = b.capture().unwrap();
    let restored = OwnerBridge::reconstruct(&saved, c.clone()).unwrap();
    assert!(value(&restored.export_native().unwrap()) == after);
    let json = b.export_native().unwrap().to_save_json().unwrap();
    let native = OwnerBridge::import_native(&json, json.len(), c.clone()).unwrap();
    assert!(value(&native.export_native().unwrap()) == after);
    for i2a in [false, true] {
        let w = world(151, 9, 2, false, Locale::En);
        let mut old = config(&w);
        old.metabolic_ticks = false;
        old.passive_ticks = i2a;
        let old = bridge(w, old);
        let saved = old.capture().unwrap();
        let restored = OwnerBridge::reconstruct(&saved, c.clone()).unwrap();
        assert!(restored.prepare_metabolic_phase().is_err());
        assert!(restored.prepare_default_cancel().is_ok());
        assert_eq!(restored.prepare_affliction_phase().is_ok(), i2a);
        assert!(value(&restored.export_native().unwrap()) == value(&old.export_native().unwrap()));
    }
}
#[test]
fn real_session_authority_refusal_preserves_and_reloads_phase_prefix() {
    let w = world(1, 10, 1, false, Locale::Uk);
    let c = config(&w);
    let b = bridge(w.clone(), c.clone());
    let mut expected = w;
    let old = expected.event_log.clone();
    let events = oracle(&mut expected);
    expected.event_log = old;
    let req = b.prepare_metabolic_phase().unwrap();
    let scope = b
        .session
        .with_runtime(&b.credential.borrow(), |s| {
            let mut keys = BTreeSet::from([s.host().publication_resource()]);
            for n in ["metabolic_hero_i2b", "metabolic_nutrition_i2b"] {
                keys.insert(s.owner::<HeroState>("Hero").unwrap().owner.resource(n)?);
            }
            for n in ["metabolic_actor_i2b", "metabolic_consequence_i2b"] {
                keys.insert(
                    s.owner::<ActorsState>("Actors")
                        .unwrap()
                        .owner
                        .resource(n)?,
                );
            }
            keys.insert(
                s.owner::<ClockState>("Clock")
                    .unwrap()
                    .owner
                    .resource("metabolic_clock_i2b")?,
            );
            keys.insert(
                s.owner::<SessionState>("Session")
                    .unwrap()
                    .owner
                    .resource("metabolic_locale_i2b")?,
            );
            Ok::<_, owners::Error>(Scope::Keys(keys))
        })
        .unwrap()
        .unwrap();
    let out = b.execute_metabolic_phase_scoped(req, scope).unwrap();
    assert_eq!(out.events, events);
    assert_eq!(out.error.as_deref(), Some("Authority"));
    assert!(value(&b.export_native().unwrap()) == value(&expected));
    let saved = b.capture().unwrap();
    let restored = OwnerBridge::reconstruct(&saved, c).unwrap();
    assert!(value(&restored.export_native().unwrap()) == value(&expected));
}
#[test]
fn missing_actor_raw_block_keeps_messages_and_nutrition_without_inventing_actor() {
    for n in [151, 1, -200] {
        let mut w = world(n, 10, 1, false, Locale::En);
        let id = w.player_id;
        w.arena.actors.remove(id);
        let mut expected = w.clone();
        let events = oracle(&mut expected);
        let b = bridge(w.clone(), config(&w));
        let out = b
            .execute_metabolic_phase(b.prepare_metabolic_phase().unwrap())
            .unwrap();
        assert!(out.error.is_none());
        assert_eq!(out.events, events);
        b.session
            .with_runtime(&b.credential.borrow(), |s| {
                let h: HeroState = decode(
                    &s.owner::<HeroState>("Hero").unwrap().owner.inspect()?,
                    PAYLOAD,
                )?;
                let a: ActorsState = decode(
                    &s.owner::<ActorsState>("Actors").unwrap().owner.inspect()?,
                    PAYLOAD,
                )?;
                let session: SessionState = decode(
                    &s.owner::<SessionState>("Session")
                        .unwrap()
                        .owner
                        .inspect()?,
                    PAYLOAD,
                )?;
                assert_eq!(h.player_nutrition, expected.player_nutrition);
                assert!(a.actors.get(id).is_none());
                assert_eq!(session.event_log, expected.event_log);
                Ok::<_, owners::Error>(())
            })
            .unwrap()
            .unwrap();
    }
}

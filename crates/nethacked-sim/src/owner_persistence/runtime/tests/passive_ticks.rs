use super::super::*;
use nethacked_types::{Locale, PetrificationState, SlimingState, TransientAfflictions};
fn world() -> SimulationWorld {
    let mut w = SimulationWorld::new_with_seed(793);
    w.hero.skills.skills.clear();
    w.arena.actors.retain(|id, _| id == w.player_id);
    w.max_event_log_len = Some(3);
    w.record_events(&[crate::GameEvent::LogMessage { text: "old".into() }]);
    w
}
fn config(w: &SimulationWorld) -> OwnerBridgeConfig {
    OwnerBridgeConfig::new(w.ruleset.clone(), w.ruleset_ref.clone())
        .unwrap()
        .with_passive_affliction_ticks()
}
fn bridge(w: SimulationWorld, c: OwnerBridgeConfig) -> OwnerBridge {
    OwnerBridge::install(NativePartitions::from_native(w), c).unwrap()
}
fn value(w: &SimulationWorld) -> serde_json::Value {
    serde_json::from_str(&w.to_save_json().unwrap()).unwrap()
}
fn legacy(w: &mut SimulationWorld) -> Vec<crate::GameEvent> {
    let mut events = vec![];
    let result = w.tick_hero_afflictions(&mut events);
    // Independent adapter oracle: exact original turn-match behavior.
    let message = match result {
        nethacked_core::afflictions::AfflictionTickResult::StoneDeath => {
            Some(nethacked_i18n::Messages::petrification_death(w.locale))
        }
        nethacked_core::afflictions::AfflictionTickResult::SlimeDeath => {
            Some(nethacked_i18n::Messages::sliming_death(w.locale))
        }
        _ => None,
    };
    if let Some(text) = message {
        if let Some(a) = w.arena.actors.get_mut(w.player_id) {
            a.is_dead = true;
        }
        events.push(crate::GameEvent::LogMessage { text: text.into() });
    }
    w.record_events(&events);
    events
}
#[test]
fn ordinary_durations_recovery_order_native_state_and_reload() {
    for duration in [1, 2, 4] {
        let mut w = world();
        w.hero.afflictions.transient = TransientAfflictions {
            confused: duration,
            stunned: duration,
            hallucinating: duration,
            blinded: duration,
            helpless: duration,
        };
        w.arena.actors[w.player_id].intrinsics.blind = true;
        let c = config(&w);
        let b = bridge(w.clone(), c.clone());
        let mut expected = w;
        for _ in 0..duration {
            let events = legacy(&mut expected);
            let out = b
                .execute_affliction_phase(b.prepare_affliction_phase().unwrap())
                .unwrap();
            assert!(out.error.is_none(), "{:?}", out.error);
            assert_eq!(out.events, events);
            assert!(value(&b.export_native().unwrap()) == value(&expected));
        }
        assert!(
            !b.export_native().unwrap().arena.actors[expected.player_id]
                .intrinsics
                .blind
        );
        let saved = b.capture().unwrap();
        let restored = OwnerBridge::reconstruct(&saved, c.clone()).unwrap();
        assert!(value(&restored.export_native().unwrap()) == value(&expected));
        let json = b.export_native().unwrap().to_save_json().unwrap();
        let native = OwnerBridge::import_native(&json, json.len(), c).unwrap();
        assert!(value(&native.export_native().unwrap()) == value(&expected));
    }
}
#[test]
fn fatal_precedence_early_return_locale_and_dead_raw_phase() {
    for locale in [Locale::En, Locale::Uk] {
        for stone in [false, true] {
            let mut w = world();
            w.locale = locale;
            if stone {
                w.hero.afflictions.petrification = Some(PetrificationState { turns_remaining: 1 });
            }
            w.hero.afflictions.sliming = Some(SlimingState { turns_remaining: 1 });
            w.hero.afflictions.transient.confused = 1;
            w.hero.afflictions.transient.blinded = 1;
            w.arena.actors[w.player_id].intrinsics.blind = true;
            let c = config(&w);
            let b = bridge(w.clone(), c);
            let mut expected = w;
            for _ in 0..2 {
                let events = legacy(&mut expected);
                let out = b
                    .execute_affliction_phase(b.prepare_affliction_phase().unwrap())
                    .unwrap();
                assert!(out.error.is_none());
                assert_eq!(out.events, events);
                assert!(value(&b.export_native().unwrap()) == value(&expected));
            }
            assert_eq!(expected.hero.afflictions.transient.confused, 1);
            if stone {
                assert_eq!(
                    expected
                        .hero
                        .afflictions
                        .sliming
                        .as_ref()
                        .unwrap()
                        .turns_remaining,
                    1
                );
            }
        }
    }
}
#[test]
fn authentic_request_replay_and_historical_profile_isolation() {
    let w = world();
    let c = config(&w);
    let b = bridge(w, c.clone());
    let req = b.prepare_affliction_phase().unwrap();
    b.execute_affliction_phase(req.clone()).unwrap();
    let after = value(&b.export_native().unwrap());
    assert!(b.execute_affliction_phase(req).is_err());
    assert!(value(&b.export_native().unwrap()) == after);
    for i1 in [false, true] {
        let w = world();
        let mut old = config(&w);
        old.passive_ticks = false;
        old.default_commands = i1;
        let old = bridge(w, old);
        let saved = old.capture().unwrap();
        let restored = OwnerBridge::reconstruct(&saved, c.clone()).unwrap();
        assert!(restored.prepare_affliction_phase().is_err());
        assert!(value(&restored.export_native().unwrap()) == value(&old.export_native().unwrap()));
        if i1 {
            assert!(restored.prepare_default_cancel().is_ok());
        }
    }
}
#[test]
fn genuine_session_authority_refusal_preserves_reloadable_phase_prefix() {
    let mut w = world();
    w.hero.afflictions.transient.confused = 1;
    let c = config(&w);
    let b = bridge(w.clone(), c.clone());
    let mut expected = w;
    let old_log = expected.event_log.clone();
    let events = legacy(&mut expected);
    expected.event_log = old_log;
    let request = b.prepare_affliction_phase().unwrap();
    let scope = b
        .session
        .with_runtime(&b.credential.borrow(), |b| {
            let mut keys = BTreeSet::from([b.host().publication_resource()]);
            for n in ["passive_hero_i2a", "passive_countdown_i2a"] {
                keys.insert(b.owner::<HeroState>("Hero").unwrap().owner.resource(n)?);
            }
            for n in ["passive_actor_i2a", "passive_status_i2a"] {
                keys.insert(
                    b.owner::<ActorsState>("Actors")
                        .unwrap()
                        .owner
                        .resource(n)?,
                );
            }
            keys.insert(
                b.owner::<SessionState>("Session")
                    .unwrap()
                    .owner
                    .resource("passive_locale_i2a")?,
            );
            Ok::<_, owners::Error>(Scope::Keys(keys))
        })
        .unwrap()
        .unwrap();
    let out = b.execute_affliction_phase_scoped(request, scope).unwrap();
    assert_eq!(out.events, events);
    assert_eq!(out.error.as_deref(), Some("Authority"));
    assert!(value(&b.export_native().unwrap()) == value(&expected));
    let saved = b.capture().unwrap();
    let restored = OwnerBridge::reconstruct(&saved, c).unwrap();
    assert!(value(&restored.export_native().unwrap()) == value(&expected));
}
#[test]
fn missing_actor_raw_phase_changes_hero_without_actor_publication_side_effects() {
    let mut w = world();
    w.hero.afflictions.transient.blinded = 1;
    let id = w.player_id;
    w.arena.actors.remove(id);
    let c = config(&w);
    let mut expected = w.clone();
    let events = legacy(&mut expected);
    let b = bridge(w, c);
    let out = b
        .execute_affliction_phase(b.prepare_affliction_phase().unwrap())
        .unwrap();
    assert!(out.error.is_none());
    assert_eq!(out.events, events);
    b.session
        .with_runtime(&b.credential.borrow(), |s| {
            let hs = s.owner::<HeroState>("Hero").unwrap().owner.inspect()?;
            let hs: HeroState = decode(&hs, PAYLOAD)?;
            assert_eq!(hs.hero.afflictions, expected.hero.afflictions);
            let a = s.owner::<ActorsState>("Actors").unwrap().owner.inspect()?;
            let a: ActorsState = decode(&a, PAYLOAD)?;
            assert!(a.actors.get(id).is_none());
            Ok::<_, owners::Error>(())
        })
        .unwrap()
        .unwrap();
}

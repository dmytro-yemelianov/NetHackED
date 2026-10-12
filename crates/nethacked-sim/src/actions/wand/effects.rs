//! Existing death and anger consequences expressed through separate authorities.
use super::WandError;
use super::{creation::birth_context, ports::*};
use crate::{
    object_timers::*,
    progress::{experience, newuexp},
    GameEvent,
};
use nethacked_arena::{ActorId, ActorRecord, ItemId, ItemLocation};
use nethacked_core::{alignlim, QuestProgress};
use nethacked_data::{
    roles::{race_stats, role_stats, Advance},
    ItemKindId, MonsterSound, MonsterSpeciesId,
};
use nethacked_types::{Alignment, Buc};

fn roll_advance<E>(
    role: &Advance,
    race: &Advance,
    level: u32,
    xlev: u32,
    rng: &mut impl Random<E>,
) -> Result<i32, E> {
    let (fix, rnd) = if level == 0 {
        (role.infix + race.infix, [role.inrnd, race.inrnd])
    } else if level < xlev {
        (role.lofix + race.lofix, [role.lornd, race.lornd])
    } else {
        (role.hifix + race.hifix, [role.hirnd, race.hirnd])
    };
    let mut value = fix;
    for r in rnd {
        if r > 0 {
            value += rng.sample(WandDraw::SignedDie(r))? as i32;
        }
    }
    Ok(value)
}
fn award_experience<E>(
    player: ActorId,
    victim: &ActorRecord,
    actors: &mut impl TryActors<E>,
    hero: &mut impl TryHero<E>,
    rng: &mut impl Random<E>,
    session: &Session<'_>,
    events: &mut Vec<GameEvent>,
) -> Result<(), WandError<E>> {
    let Some(def) = session.ruleset.monster_for_actor(victim) else {
        return Ok(());
    };
    hero.credit_experience(experience(
        victim.level,
        victim.ac,
        def.speed,
        &def.attacks,
        def.has_flag("nasty"),
        def.id == Some(MonsterSpeciesId::MAIL_DAEMON),
    ))?;
    let level = actors.actor(player)?.map_or(1, |p| p.level);
    if level >= 30 || hero.experience()? < newuexp(level) {
        return Ok(());
    }
    let Some(role) = role_stats(hero.role_name()?).or_else(|| role_stats("valkyrie")) else {
        return Ok(());
    };
    let race = race_stats(hero.race()?);
    let mut hp = roll_advance(&role.hp, &race.hp, level, role.xlev, rng)?;
    if level > 0 {
        hp += match hero.constitution()? {
            ..=3 => -2,
            4..=6 => -1,
            7..=14 => 0,
            15..=16 => 1,
            17 => 2,
            18 => 3,
            _ => 4,
        };
    }
    let hp = hp.max(1);
    let energy = roll_advance(&role.energy, &race.energy, level, role.xlev, rng)?;
    let energy = match hero.role_name()?.to_ascii_lowercase().as_str() {
        "priest" | "cleric" | "wizard" => 2 * energy,
        "healer" | "knight" => (3 * energy) / 2,
        "barbarian" | "valkyrie" => (3 * energy) / 4,
        _ => energy,
    }
    .max(0);
    let cap = newuexp(level + 1);
    let experience = if hero.experience()? >= cap {
        cap - 1
    } else {
        hero.experience()?
    };
    let back = if hero.level_max()? < level + 1 {
        ""
    } else {
        "back "
    };
    hero.record_level_gain(level, hp, energy, experience)?;
    actors.gain_level(player, hp as u32, level + 1)?;
    events.push(GameEvent::LogMessage {
        text: format!("Welcome {back}to experience level {}.", level + 1),
    });
    Ok(())
}
fn attach<E>(
    tree: nethacked_data::containers::GeneratedItem,
    location: ItemLocation,
    objects: &mut impl TryObjects<E>,
    clock: &mut impl TryClock<E>,
) -> Result<ItemId, WandError<E>> {
    use nethacked_data::corpses::{CorpseTimeoutRecipe as Recipe, CorpseTimerOperation as Op};
    let mut tree = tree;
    tree.item.location = location;
    let id = objects.insert_generated(tree.item)?;
    if tree.corpse_timer_operations.is_empty() {
        if let Some(recipe) = tree.corpse_timeout {
            tree.corpse_timer_operations.push(Op::Start(recipe));
        }
    }
    let target = ObjectTimerTarget::Active(id);
    for operation in tree.corpse_timer_operations {
        match operation {
            Op::Cancel => clock.cancel_corpse_timers(target)?,
            Op::Start(recipe) => {
                let (sampled_at, delay, kind) = match recipe {
                    Recipe::Rot { sampled_at, delay } => {
                        (sampled_at, delay, ObjectTimerKind::RotCorpse)
                    }
                    Recipe::Revive { sampled_at, delay } => {
                        (sampled_at, delay, ObjectTimerKind::ReviveMonster)
                    }
                    Recipe::UnsupportedRider => continue,
                };
                clock
                    .start_corpse_timer(
                        target,
                        kind,
                        sampled_at.checked_add_signed(delay),
                        objects,
                    )?
                    .map_err(WandError::Timer)?;
            }
        }
    }
    if objects.frozen(id)? {
        clock.cancel_corpse_timers(target)?;
    }
    for child in tree.contents {
        attach(child, ItemLocation::InContainer(id), objects, clock)?;
    }
    Ok(id)
}
#[allow(clippy::too_many_arguments)]
pub(super) fn killed<E>(
    player: ActorId,
    defender_id: ActorId,
    objects: &mut impl TryObjects<E>,
    actors: &mut impl TryActors<E>,
    hero: &mut impl TryHero<E>,
    clock: &mut impl TryClock<E>,
    rng: &mut impl Random<E>,
    session: &Session<'_>,
    events: &mut Vec<GameEvent>,
) -> Result<(), WandError<E>> {
    let Some(defender) = actors.actor(defender_id)? else {
        return Ok(());
    };
    hero.record_kill()?;
    award_experience(player, &defender, actors, hero, rng, session, events)?;
    events.push(GameEvent::LogMessage {
        text: nethacked_i18n::Messages::killed(&defender.name, session.locale),
    });
    if defender_id == player {
        return Ok(());
    }
    let quest = nethacked_core::get_role_quest_config_or_default(hero.role_name()?);
    let def = session.ruleset.monster_for_actor(&defender);
    let leader = defender.name.eq_ignore_ascii_case(quest.leader_name);
    let nemesis = defender.name.eq_ignore_ascii_case(quest.nemesis_name)
        || def.is_some_and(|m| m.msound == MonsterSound::Nemesis);
    let align_nemesis = def.is_some_and(|m| m.msound == MonsterSound::Nemesis);
    let guardian = defender.name.eq_ignore_ascii_case(quest.guardian_name)
        || def.is_some_and(|m| m.msound == MonsterSound::Guardian);
    let priest =
        def.is_some_and(|m| m.is_priest()) || defender.name.to_ascii_lowercase().contains("priest");
    let limit = alignlim(clock.turn()?);
    let alignment = actors
        .actor(player)?
        .map(|p| p.alignment)
        .unwrap_or(Alignment::Neutral);
    if leader {
        let penalty = -(hero.alignment_record()? + limit / 2);
        hero.adjust_alignment(penalty, limit)?;
        hero.adjust_favor(-7)?;
        hero.adjust_luck(-20)?;
        hero.mark_quest_leader_killed()?;
        events.push(GameEvent::LogMessage {
            text: nethacked_i18n::Messages::bad_idea(
                hero.quest_progress()? == QuestProgress::Completed,
                session.locale,
            )
            .into(),
        });
        for id in actors.guardians(quest.guardian_name, true, session.ruleset)? {
            if id == player {
                continue;
            }
            if let Some(mon) = actors.actor(id)? {
                if mon.is_peaceful && !mon.is_tame {
                    actors.make_hostile(id)?;
                    hero.adjust_alignment(-1, limit)?;
                }
            }
        }
    } else if align_nemesis {
        if !hero.quest_leader_killed()? {
            hero.adjust_alignment(limit / 4, limit)?;
        }
    } else if guardian {
        hero.adjust_alignment(-(limit / 8), limit)?;
        hero.adjust_favor(-1)?;
        hero.adjust_luck(-4)?;
        events.push(GameEvent::LogMessage {
            text: nethacked_i18n::Messages::bad_idea(true, session.locale).into(),
        });
    } else if priest {
        let coaligned = defender.alignment == alignment;
        hero.adjust_alignment(if coaligned { -2 } else { 2 }, limit)?;
        if coaligned {
            hero.clear_divine_protection()?;
        }
        if def.map(|m| m.maligntyp).unwrap_or(0) == nethacked_core::peace::A_NONE
            || defender.name.to_ascii_lowercase().contains("moloch")
        {
            hero.adjust_alignment(limit / 4, limit)?;
        }
    } else if defender.is_tame {
        hero.adjust_alignment(-15, limit)?;
        events.push(GameEvent::LogMessage {
            text: nethacked_i18n::Messages::distant_thunder(session.locale).into(),
        });
    } else if defender.is_peaceful {
        hero.adjust_alignment(-5, limit)?;
    }
    hero.adjust_alignment(defender.malign, limit)?;
    if nemesis {
        hero.defeat_quest_nemesis()?;
        let artifact = match quest.role_name.to_lowercase().as_str() {
            "valkyrie" => ItemKindId::ART_ORB_OF_FATE,
            "wizard" => ItemKindId::ART_EYE_OF_THE_AETHIOPICA,
            "barbarian" => ItemKindId::ART_HEART_OF_AHRIMAN,
            "knight" => ItemKindId::ART_MAGIC_MIRROR_OF_MERLIN,
            "monk" => ItemKindId::ART_EYES_OF_THE_OVERWORLD,
            "rogue" => ItemKindId::ART_MASTER_KEY_OF_THIEVERY,
            "tourist" => ItemKindId::ART_YENDORIAN_EXPRESS_CARD,
            "healer" => ItemKindId::ART_STAFF_OF_AESCULAPIUS,
            _ => ItemKindId::ART_ORB_OF_DETECTION,
        };
        if let Some(item) = session.ruleset.create_item_record_by_id(
            artifact,
            ItemLocation::Floor(defender.coord),
            Buc::Blessed,
        ) {
            objects.insert_generated(item)?;
        }
        events.push(GameEvent::LogMessage {
            text: nethacked_i18n::Messages::quest_nemesis_defeat(
                quest.nemesis_name,
                quest.artifact_name,
                session.locale,
            ),
        });
    }
    let (ctx, monster_ctx) = birth_context(
        actors.actor(player)?.as_ref(),
        clock.turn()?,
        session.branch,
        session.depth,
        false,
    );
    let gone = actors.genocide_filter()?;
    if let Some(corpse) = session.ruleset.try_create_initialized_corpse_for_actor(
        &defender,
        ItemLocation::Floor(defender.coord),
        ctx,
        &monster_ctx,
        |d| gone(&d.name, d.glyph),
        |n| rng.sample(WandDraw::Below(n)),
    )? {
        attach(corpse, ItemLocation::Floor(defender.coord), objects, clock)?;
    }
    Ok(())
}
#[allow(clippy::too_many_arguments)]
pub(super) fn anger<E>(
    player_id: ActorId,
    target_id: ActorId,
    actors: &mut impl TryActors<E>,
    geography: &mut impl TryGeography<E>,
    hero: &mut impl TryHero<E>,
    clock: &impl TryClock<E>,
    rng: &mut impl Random<E>,
    session: &Session<'_>,
    events: &mut Vec<GameEvent>,
) -> Result<(), WandError<E>> {
    let Some(target) = actors.actor(target_id)? else {
        return Ok(());
    };
    let Some(player) = actors.actor(player_id)? else {
        return Ok(());
    };
    let limit = alignlim(clock.turn()?);
    let engraving = geography
        .engraving(player.coord)?
        .filter(|e| e.text == "Elbereth");
    if let Some(e) = engraving.as_ref() {
        let arch = session.ruleset.monster_for_actor(&target);
        let exempt = nethacked_core::engraving::onscary_exempt(
            arch.is_some_and(|a| a.is_human),
            false,
            arch.is_some_and(|a| a.id == Some(MonsterSpeciesId::SHOPKEEPER)),
            false,
        );
        let scared = nethacked_core::engraving::is_elbereth_ward_active(
            Some(e),
            target.intrinsics.blind,
            target.is_unique,
            target.is_peaceful,
            exempt,
        );
        if scared || target.is_peaceful {
            events.push(GameEvent::LogMessage {
                text: nethacked_i18n::Messages::feel_hypocrite(session.locale).into(),
            });
            let n = if hero.alignment_record()? > 5 {
                -5
            } else {
                -(rng.sample(WandDraw::SignedDie(5))? as i32)
            };
            hero.adjust_alignment(n, limit)?;
            if !player.intrinsics.blind {
                events.push(GameEvent::LogMessage {
                    text: nethacked_i18n::Messages::engraving_fades(session.locale).into(),
                });
            }
            geography.erase_engraving(player.coord)?;
        }
    }
    if !target.is_peaceful || target.is_tame {
        return Ok(());
    }
    actors.make_hostile(target_id)?;
    let priest = session
        .ruleset
        .monster_for_actor(&target)
        .is_some_and(|a| a.id == Some(MonsterSpeciesId::ALIGNED_CLERIC));
    let delta = if priest {
        if target.alignment == player.alignment {
            -5
        } else {
            2
        }
    } else {
        -1
    };
    hero.adjust_alignment(delta, limit)?;
    events.push(GameEvent::LogMessage {
        text: nethacked_i18n::Messages::gets_angry(
            &target.name,
            crate::peace::monnam_article(session.ruleset, &target.name),
            session.locale,
        ),
    });
    let quest = nethacked_core::get_role_quest_config_or_default(hero.role_name()?);
    if target.name.eq_ignore_ascii_case(quest.leader_name) {
        let mut got_mad = 0u32;
        for id in actors.guardians(quest.guardian_name, false, session.ruleset)? {
            if actors.actor(id)?.is_some_and(|mon| mon.is_peaceful) {
                actors.make_hostile(id)?;
                got_mad += 1;
            }
        }
        if got_mad > 0 {
            events.push(GameEvent::LogMessage {
                text: nethacked_i18n::Messages::guardians_angry_too(
                    quest.guardian_name,
                    got_mad > 1,
                    session.locale,
                ),
            });
        }
    }
    Ok(())
}

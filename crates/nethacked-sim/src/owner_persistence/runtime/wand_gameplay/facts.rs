use super::*;
fn bounded<T>(v: Vec<T>) -> owners::Result<Vec<T>> {
    if v.len() > MAX_REFERENCES {
        Err(owners::Error::Limit)
    } else {
        Ok(v)
    }
}
pub(crate) fn objects(
    s: &ObjectsState,
    q: &ObjectQuery,
    c: &OwnerBridgeConfig,
) -> owners::Result<FactValue> {
    Ok(match q {
        ObjectQuery::Carried(actor) => FactValue::Items(bounded(
            s.items
                .iter()
                .filter_map(|(id, item)| {
                    (item.location == nethacked_arena::ItemLocation::CarriedBy(*actor))
                        .then_some(id)
                })
                .collect(),
        )?),
        ObjectQuery::Selected(id) => FactValue::Selected(s.items.get(*id).map(|item| Selected {
            class: item.class,
            name: item.name.clone(),
            definition: item.definition.clone(),
            buc: item.buc,
            enchantment: item.enchantment,
        })),
        ObjectQuery::Floor { coord, kind } => FactValue::Bool(s.items.iter().any(|(_, item)| {
            item.location == nethacked_arena::ItemLocation::Floor(*coord)
                && c.ruleset
                    .item(&item.name)
                    .is_some_and(|d| d.id == Some(*kind))
        })),
        ObjectQuery::Amulet(actor) => FactValue::Bool(s.items.iter().any(|(_, item)| {
            item.location == nethacked_arena::ItemLocation::CarriedBy(*actor)
                && crate::actions::items::is_real_amulet(item)
        })),
        ObjectQuery::Exists(target) => FactValue::Bool(match target.residency {
            Residency::Active => s.items.contains_key(target.id),
            Residency::Cached { .. } => return Err(owners::Error::Unsupported),
        }),
    })
}
pub(crate) fn actors(
    s: &ActorsState,
    q: &ActorQuery,
    c: &OwnerBridgeConfig,
) -> owners::Result<FactValue> {
    Ok(match q {
        ActorQuery::Record(id) => {
            let actor = s.actors.get(*id).cloned();
            if actor
                .as_ref()
                .is_some_and(|a| a.abilities.len() > MAX_TARGETS || a.name.len() > MAX_TEXT_BYTES)
            {
                return Err(owners::Error::Limit);
            }
            FactValue::Actor(actor)
        }
        ActorQuery::At(coord) => FactValue::At(
            s.actors
                .iter()
                .find_map(|(id, a)| (!a.is_dead && a.coord == *coord).then_some(id)),
        ),
        ActorQuery::Guardians {
            name,
            include_sound,
        } => FactValue::Actors(bounded(
            s.actors
                .iter()
                .filter_map(|(id, a)| {
                    (!a.is_dead
                        && (a.name.eq_ignore_ascii_case(name)
                            || (*include_sound
                                && c.ruleset.monster_for_actor(a).is_some_and(|d| {
                                    d.msound == nethacked_data::MonsterSound::Guardian
                                }))))
                    .then_some(id)
                })
                .collect(),
        )?),
    })
}
pub(crate) fn genocide(s: &HeroState, version: u32, offset: u32) -> owners::Result<FactValue> {
    if version != 3 {
        return Err(owners::Error::Schema);
    }
    let species: BTreeSet<_> = s.genocide_registry.genocided_species.iter().collect();
    let classes: BTreeSet<_> = s.genocide_registry.genocided_classes.iter().collect();
    let offset = offset as usize;
    let total = species.len() + classes.len();
    if offset > total {
        return Err(owners::Error::Schema);
    }
    let end = (offset + MAX_REFERENCES).min(total);
    Ok(FactValue::Genocide {
        species: species
            .iter()
            .enumerate()
            .filter_map(|(n, s)| (n >= offset && n < end).then_some((*s).clone()))
            .collect(),
        classes: classes
            .iter()
            .enumerate()
            .filter_map(|(n, g)| {
                (n + species.len() >= offset && n + species.len() < end).then_some(**g)
            })
            .collect(),
        next: (end < total).then_some(end as u32),
    })
}
pub(crate) fn sample(
    s: &mut RandomState,
    reservation: &RandomReservation,
    draw: &SemanticDraw,
    c: &OwnerBridgeConfig,
) -> owners::Result<OperationResult> {
    if !c.wand_gameplay {
        return Err(owners::Error::Unsupported);
    }
    if reservation.algorithm_version != 1
        || reservation.max_draws != 1
        || reservation.stream != s.rng.get_stream()
        || reservation.word_position != s.rng.get_word_pos()
    {
        return Err(owners::Error::Stale);
    }
    let draw = draw.draw()?;
    Ok(OperationResult::Sample(draw.sample(&mut s.rng)))
}

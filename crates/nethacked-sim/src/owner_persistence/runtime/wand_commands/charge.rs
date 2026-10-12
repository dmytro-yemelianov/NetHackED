//! Opt-in zero-charge publication. Random previews/samples locally; Objects owns charges.
use super::*;
use nethacked_core::artifacts_wands::{wand_charge_attempt, WandChargeAttempt};
const POLICY: [u8; 32] = [249; 32];
fn preview(obs: &[Observation]) -> owners::Result<(RandomReservation, u32)> {
    for o in obs {
        if let WandFacts::Command(CommandFacts::WrestPreview { reservation, draw }) =
            decode(o.bytes(), MAX_RESPONSE_BYTES)?
        {
            return Ok((reservation, draw));
        }
    }
    Err(owners::Error::Schema)
}
fn result(expected: i8, obs: &[Observation]) -> owners::Result<OperationResult> {
    let draw = if expected == 0 {
        Some(preview(obs)?.1)
    } else {
        None
    };
    let law = wand_charge_attempt(expected, draw).map_err(|_| owners::Error::Invariant)?;
    Ok(match law {
        WandChargeAttempt::Ready { spe, wrested } => OperationResult::ChargeAttempt {
            remaining: spe,
            ready: true,
            wrested,
        },
        WandChargeAttempt::Unzappable { spe } => OperationResult::ChargeAttempt {
            remaining: spe,
            ready: false,
            wrested: false,
        },
    })
}
pub(super) fn apply(
    s: &mut ObjectsState,
    target: ObjectRef,
    expected: i8,
    obs: &[Observation],
    c: &OwnerBridgeConfig,
) -> owners::Result<OperationResult> {
    if !c.wand_charge_attempts || target.residency != Residency::Active {
        return Err(owners::Error::Unsupported);
    }
    let first = obs.first().ok_or(owners::Error::Schema)?;
    let WandFacts::Hero(hero) = decode(first.bytes(), MAX_RESPONSE_BYTES)? else {
        return Err(owners::Error::Schema);
    };
    let item = s.items.get(target.id).ok_or(owners::Error::Stale)?;
    if item.class != nethacked_types::ItemClass::Wand
        || item.buc != nethacked_types::Buc::Uncursed
        || item.enchantment != expected
        || item.location != nethacked_arena::ItemLocation::CarriedBy(hero.player)
        || hero.polymorph.is_some()
    {
        return Err(owners::Error::Invariant);
    }
    let def = match &item.definition {
        Some(id) => c.ruleset.item_for_definition(id),
        None => c.ruleset.item(&item.name),
    }
    .ok_or(owners::Error::Schema)?;
    if def.id != Some(nethacked_data::ItemKindId::WAN_FIRE) {
        return Err(owners::Error::Unsupported);
    }
    let out = result(expected, obs)?;
    if let OperationResult::ChargeAttempt {
        remaining,
        ready: true,
        ..
    } = out
    {
        s.items
            .get_mut(target.id)
            .ok_or(owners::Error::Stale)?
            .enchantment = remaining;
    }
    Ok(out)
}
struct ChargeAccounting;
impl Accounting for ChargeAccounting {
    fn validate(&self, p: &[ProposalView], obs: &[Observation]) -> owners::Result<()> {
        let object = obs
            .iter()
            .find_map(
                |o| match decode::<WandFacts>(o.bytes(), MAX_RESPONSE_BYTES) {
                    Ok(WandFacts::Objects(f)) => Some(f),
                    _ => None,
                },
            )
            .ok_or(owners::Error::Schema)?;
        let expected_charge = object.charges;
        let law = result(expected_charge, obs)?;
        let ready = matches!(law, OperationResult::ChargeAttempt { ready: true, .. });
        let random = p.iter().find(|p| p.receipt.command == COMMANDS[5].1);
        let objects = p.iter().find(|p| p.receipt.command == COMMANDS[0].1);
        if p.len() != usize::from(expected_charge == 0) + usize::from(ready)
            || p.iter().any(|p| !p.events.is_empty())
        {
            return Err(owners::Error::Invariant);
        }
        if expected_charge == 0 {
            let (_, draw) = preview(obs)?;
            let r = random.ok_or(owners::Error::Invariant)?;
            if !matches!(decode::<OperationResult>(&r.reply,MAX_RESPONSE_BYTES)?,OperationResult::Sample(x) if x==draw)
            {
                return Err(owners::Error::Invariant);
            }
        } else if random.is_some() {
            return Err(owners::Error::Invariant);
        }
        if ready {
            let o = objects.ok_or(owners::Error::Invariant)?;
            if encode(&law, MAX_RESPONSE_BYTES)? != o.reply {
                return Err(owners::Error::Invariant);
            }
            let before: ObjectsState = decode(&o.before, PAYLOAD)?;
            let after: ObjectsState = decode(&o.proposed, PAYLOAD)?;
            let mut expected = before;
            let OperationResult::ChargeAttempt { remaining, .. } = law else {
                return Err(owners::Error::Invariant);
            };
            expected
                .items
                .get_mut(object.target.id)
                .ok_or(owners::Error::Stale)?
                .enchantment = remaining;
            if encode(&expected, PAYLOAD)? != encode(&after, PAYLOAD)? {
                return Err(owners::Error::Invariant);
            }
        } else if objects.is_some() {
            return Err(owners::Error::Invariant);
        }
        Ok(())
    }
}
pub(super) fn execute(
    r: &SourceRoutes,
    target: ObjectRef,
    expected: i8,
    immediate: Vec<Observation>,
    mut provenance: Vec<Observation>,
    view: &CallerView,
    c: &OwnerBridgeConfig,
) -> owners::Result<Vec<u8>> {
    // Validate against owned current object even when failed/negative attempts do not write Objects.
    let guard_query = WandQuery::Objects(ObjectRequest::Identity(target));
    let (guard, nested) = r.observe(&guard_query, view)?;
    provenance.extend(preflight(r, target, expected, view, c)?);
    provenance.extend(nested);
    provenance.push(guard);
    let law = result(expected, &immediate)?;
    let ready = matches!(law, OperationResult::ChargeAttempt { ready: true, .. });
    if expected < 0 {
        return encode(&law, MAX_RESPONSE_BYTES);
    }
    let mut candidates = vec![];
    let mut ids = vec![];
    if expected == 0 {
        let (reservation, _) = preview(&immediate)?;
        let request = WandOperation {
            expected_owner: OwnerWitness::from(&r.random.stamp()?),
            dependencies: vec![],
            command: WandOwnerCommand::Sample {
                reservation,
                draw: super::super::wand_gameplay::SemanticDraw::Below(121),
            },
        };
        candidates.push(
            r.random
                .prepare(
                    COMMANDS[5].1,
                    &encode(&request, MAX_REQUEST_BYTES)?,
                    vec![],
                    view,
                )?
                .erase(),
        );
        ids.push(r.random.stamp()?.owner);
    }
    if ready {
        let request = WandOperation {
            expected_owner: OwnerWitness::from(&r.objects.stamp()?),
            dependencies: vec![],
            command: WandOwnerCommand::Objects(ObjectOperation::ChargeAttempt { target, expected }),
        };
        candidates.push(
            r.objects
                .prepare(
                    COMMANDS[0].1,
                    &encode(&request, MAX_REQUEST_BYTES)?,
                    immediate,
                    view,
                )?
                .erase(),
        );
        ids.push(r.objects.stamp()?.owner);
    }
    let batch = r.host.batch(
        candidates,
        provenance,
        &ids,
        ids.clone(),
        POLICY,
        Rc::new(ChargeAccounting),
        profile_capability(c, COMMANDS[0].1),
        view,
    )?;
    let receipt = batch.commit(view)?;
    #[cfg(test)]
    {
        let states = (r.objects.inspect()?, r.random.inspect()?);
        let history = r.host.history()?;
        assert_eq!(history.last(), Some(&receipt));
        assert_eq!(
            receipt.participants.len(),
            usize::from(expected == 0) + usize::from(ready)
        );
        assert_eq!(batch.commit(view), Err(owners::Error::Stale));
        assert_eq!((r.objects.inspect()?, r.random.inspect()?), states);
        assert_eq!(r.host.history()?, history);
    }
    #[cfg(not(test))]
    let _ = receipt;
    encode(&law, MAX_RESPONSE_BYTES)
}

fn identity() -> op::ArtifactIdentity {
    artifact("native-wand-charge-accounting-i5c", 249)
}
pub(super) fn capture(
    a: &mut op::AccountingCaptureCatalog,
    c: &OwnerBridgeConfig,
) -> owners::Result<()> {
    if c.wand_charge_attempts {
        a.register(identity(), POLICY, Rc::new(ChargeAccounting))?;
    }
    Ok(())
}
pub(super) fn restore(
    a: &mut op::AccountingRestoreCatalog,
    c: &OwnerBridgeConfig,
) -> owners::Result<()> {
    if c.wand_charge_attempts {
        a.allow(identity(), POLICY, || Ok(Rc::new(ChargeAccounting)))?;
    }
    Ok(())
}
pub(super) fn preflight(
    r: &SourceRoutes,
    target: ObjectRef,
    expected: i8,
    view: &CallerView,
    c: &OwnerBridgeConfig,
) -> owners::Result<Vec<Observation>> {
    if !c.wand_charge_attempts || target.residency != Residency::Active {
        return Err(owners::Error::Unsupported);
    }
    let mut guards = vec![];
    let (o, nested) = r.observe(&WandQuery::Objects(ObjectRequest::Identity(target)), view)?;
    let WandFacts::Objects(item) = decode(o.bytes(), MAX_RESPONSE_BYTES)? else {
        return Err(owners::Error::Schema);
    };
    guards.extend(nested);
    guards.push(o);
    let (h, nested) = r.observe(&WandQuery::Hero, view)?;
    let WandFacts::Hero(hero) = decode(h.bytes(), MAX_RESPONSE_BYTES)? else {
        return Err(owners::Error::Schema);
    };
    guards.extend(nested);
    guards.push(h);
    if item.charges != expected {
        return Err(owners::Error::Stale);
    }
    if item.buc != nethacked_types::Buc::Uncursed
        || item.class != nethacked_types::ItemClass::Wand
        || item.resolved_kind != Some(nethacked_data::ItemKindId::WAN_FIRE)
        || item.placement != nethacked_arena::ItemLocation::CarriedBy(hero.player)
        || hero.polymorph.is_some()
    {
        return Err(owners::Error::Unsupported);
    }
    let (a, nested) = r.observe(
        &WandQuery::Actors(ActorsRequest {
            residency: Residency::Active,
            origin: hero.player,
            targets: vec![],
        }),
        view,
    )?;
    let WandFacts::Actors(actors) = decode(a.bytes(), MAX_RESPONSE_BYTES)? else {
        return Err(owners::Error::Schema);
    };
    guards.extend(nested);
    guards.push(a);
    let (g, nested) = r.observe(
        &WandQuery::Geography(GeographyRequest {
            residency: Residency::Active,
            cells: vec![actors.origin.coord],
            trajectory: None,
        }),
        view,
    )?;
    let WandFacts::Geography(geo) = decode(g.bytes(), MAX_RESPONSE_BYTES)? else {
        return Err(owners::Error::Schema);
    };
    guards.extend(nested);
    guards.push(g);
    if geo.cells.iter().any(|cell| {
        matches!(
            cell.tile,
            nethacked_types::Tile::Pool { .. }
                | nethacked_types::Tile::Moat
                | nethacked_types::Tile::Lava
        )
    }) {
        return Err(owners::Error::Unsupported);
    }
    Ok(guards)
}

pub(super) fn guard_bridge(
    b: &OwnerBridge,
    target: ObjectRef,
    expected: i8,
) -> Result<(), OwnerBridgeError> {
    if !b.config.wand_charge_attempts || target.residency != Residency::Active {
        return Err(owners::Error::Unsupported.into());
    }
    let read = |query| {
        b.observe_wand(WandRead {
            query,
            expected_owner: None,
        })
        .map(|o| o.facts)
    };
    let WandFacts::Objects(item) = read(WandQuery::Objects(ObjectRequest::Identity(target)))?
    else {
        return Err(owners::Error::Schema.into());
    };
    let WandFacts::Hero(hero) = read(WandQuery::Hero)? else {
        return Err(owners::Error::Schema.into());
    };
    if item.charges != expected {
        return Err(owners::Error::Stale.into());
    }
    if item.buc != nethacked_types::Buc::Uncursed
        || item.class != nethacked_types::ItemClass::Wand
        || item.resolved_kind != Some(nethacked_data::ItemKindId::WAN_FIRE)
        || item.placement != nethacked_arena::ItemLocation::CarriedBy(hero.player)
        || hero.polymorph.is_some()
    {
        return Err(owners::Error::Unsupported.into());
    }
    let WandFacts::Actors(a) = read(WandQuery::Actors(ActorsRequest {
        residency: Residency::Active,
        origin: hero.player,
        targets: vec![],
    }))?
    else {
        return Err(owners::Error::Schema.into());
    };
    let WandFacts::Geography(g) = read(WandQuery::Geography(GeographyRequest {
        residency: Residency::Active,
        cells: vec![a.origin.coord],
        trajectory: None,
    }))?
    else {
        return Err(owners::Error::Schema.into());
    };
    if g.cells.iter().any(|cell| {
        matches!(
            cell.tile,
            nethacked_types::Tile::Pool { .. }
                | nethacked_types::Tile::Moat
                | nethacked_types::Tile::Lava
        )
    }) {
        return Err(owners::Error::Unsupported.into());
    }
    Ok(())
}

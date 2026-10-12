//! Tagged raw dressing on the existing frame capabilities only.
use super::default_commands::{self as frame, Facts};
use super::*;
const POLICY: [u8; 32] = [111; 32];
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DressingRequest {
    dressing_i2d: bool,
    guards: Vec<wand::OwnerWitness>,
}
pub use super::default_commands::DefaultReply as DressingReply;
pub(super) fn tagged(request: &[u8]) -> bool {
    serde_json::from_slice::<serde_json::Value>(request)
        .ok()
        .is_some_and(|v| v.get("dressing_i2d") == Some(&serde_json::Value::Bool(true)))
}
fn command(request: &[u8]) -> owners::Result<()> {
    if !tagged(request) {
        return Err(owners::Error::Unsupported);
    }
    Ok(())
}
fn result(
    e: &crate::actions::equipment::EquipmentState,
    obs: &[Observation],
) -> owners::Result<crate::passive_dressing::Result> {
    if obs.len() != 4 {
        return Err(owners::Error::Schema);
    }
    let (player, _, protection) = frame::hero(&obs[0])?;
    let Facts::Actor(alive) = frame::fact(&obs[2])? else {
        return Err(owners::Error::Schema);
    };
    let Facts::Equipment(original) = frame::fact(&obs[3])? else {
        return Err(owners::Error::Schema);
    };
    if &original != e {
        return Err(owners::Error::Stale);
    }
    let mut ids = e.armor.iter().map(|(_, id)| *id).collect::<Vec<_>>();
    if let Some(d) = &e.dressing {
        ids.push(d.item);
    }
    ids.sort();
    ids.dedup();
    let Facts::Items(entries) = frame::fact(&obs[1])? else {
        return Err(owners::Error::Schema);
    };
    if ids.len() > frame::MAX || entries.iter().map(|(id, _)| *id).collect::<Vec<_>>() != ids {
        return Err(owners::Error::Invariant);
    }
    Ok(crate::passive_dressing::tick(
        e,
        &frame::items(&obs[1])?,
        player,
        alive == Some(false),
        protection,
    ))
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LocalRequest {
    dressing_i2d: bool,
    equipment: crate::actions::equipment::EquipmentState,
    alive: bool,
}
pub(super) fn prepare_equipment(
    s: &EquipmentState,
    name: &str,
    request: &[u8],
    obs: &[Observation],
) -> owners::Result<Proposal<EquipmentState>> {
    command(request)?;
    if name != frame::WRITE_EQ || obs.len() != 2 {
        return Err(owners::Error::Unsupported);
    }
    let req: LocalRequest = decode(request, wand::MAX_REQUEST_BYTES)?;
    let (player, _, protection) = frame::hero(&obs[0])?;
    let out = crate::passive_dressing::tick(
        &s.equipment,
        &frame::items(&obs[1])?,
        player,
        req.alive,
        protection,
    );
    let mut next = s.clone();
    next.equipment = out.equipment;
    Ok(Proposal {
        next,
        events: out
            .events
            .iter()
            .map(|e| encode(e, wand::MAX_RESPONSE_BYTES))
            .collect::<owners::Result<_>>()?,
        reply: b"null".to_vec(),
    })
}
pub(super) fn prepare_actors(
    s: &ActorsState,
    name: &str,
    request: &[u8],
    obs: &[Observation],
) -> owners::Result<Proposal<ActorsState>> {
    command(request)?;
    if name != frame::WRITE_AC || obs.len() != 2 {
        return Err(owners::Error::Unsupported);
    }
    let req: LocalRequest = decode(request, wand::MAX_REQUEST_BYTES)?;
    let (player, _, protection) = frame::hero(&obs[0])?;
    let alive = s.actors.get(player).is_some_and(|a| !a.is_dead);
    let out = crate::passive_dressing::tick(
        &req.equipment,
        &frame::items(&obs[1])?,
        player,
        alive,
        protection,
    );
    let mut next = s.clone();
    if let Some(ac) = out.ac {
        next.actors.get_mut(player).ok_or(owners::Error::Stale)?.ac = ac;
    }
    Ok(Proposal {
        next,
        events: vec![],
        reply: b"null".to_vec(),
    })
}
#[cfg(test)]
thread_local! {static WRONG_ALIVE:std::cell::Cell<bool>=const{std::cell::Cell::new(false)};static STALE_REFUSALS:std::cell::Cell<u32>=const{std::cell::Cell::new(0)};static REFUSALS:std::cell::Cell<u32>=const{std::cell::Cell::new(0)};}
#[cfg(test)]
pub(super) fn wrong_alive(on: bool) {
    WRONG_ALIVE.with(|x| x.set(on));
}
#[cfg(test)]
pub(super) fn refusals() -> u32 {
    REFUSALS.with(|x| x.get())
}
#[cfg(test)]
pub(super) fn stale_refusals() -> u32 {
    STALE_REFUSALS.with(|x| x.get())
}
struct DressingAccounting;
impl Accounting for DressingAccounting {
    fn validate(&self, p: &[ProposalView], obs: &[Observation]) -> owners::Result<()> {
        if p.len() != 2 || obs.len() != 4 {
            return Err(owners::Error::Invariant);
        }
        let e = p
            .iter()
            .find(|p| p.receipt.command == frame::WRITE_EQ)
            .ok_or(owners::Error::Invariant)?;
        let a = p
            .iter()
            .find(|p| p.receipt.command == frame::WRITE_AC)
            .ok_or(owners::Error::Invariant)?;
        let equipment: EquipmentState = decode(&e.before, PAYLOAD)?;
        let actors: ActorsState = decode(&a.before, PAYLOAD)?;
        let out = result(&equipment.equipment, obs)?;
        let (player, _, _) = frame::hero(&obs[0])?;
        let Facts::Actor(dead) = frame::fact(&obs[2])? else {
            return Err(owners::Error::Schema);
        };
        if actors.actors.get(player).map(|a| a.is_dead) != dead {
            return Err(owners::Error::Stale);
        }
        let mut expected_equipment = equipment;
        expected_equipment.equipment = out.equipment;
        let mut expected_actors = actors;
        if let Some(ac) = out.ac {
            expected_actors
                .actors
                .get_mut(player)
                .ok_or(owners::Error::Stale)?
                .ac = ac;
        }
        let events = out
            .events
            .iter()
            .map(|e| encode(e, wand::MAX_RESPONSE_BYTES))
            .collect::<owners::Result<Vec<_>>>()?;
        if encode(&expected_equipment, PAYLOAD)? != e.proposed
            || encode(&expected_actors, PAYLOAD)? != a.proposed
            || e.events != events
            || !a.events.is_empty()
            || e.reply != b"null"
            || a.reply != b"null"
        {
            #[cfg(test)]
            REFUSALS.with(|x| x.set(x.get() + 1));
            return Err(owners::Error::Invariant);
        }
        Ok(())
    }
}
fn identity() -> op::ArtifactIdentity {
    artifact("native-dressing-i2d-accounting", 111)
}
pub(super) fn capture(
    a: &mut op::AccountingCaptureCatalog,
    c: &OwnerBridgeConfig,
) -> owners::Result<()> {
    if c.dressing_ticks {
        a.register(identity(), POLICY, Rc::new(DressingAccounting))?;
    }
    Ok(())
}
pub(super) fn restore(
    a: &mut op::AccountingRestoreCatalog,
    c: &OwnerBridgeConfig,
) -> owners::Result<()> {
    if c.dressing_ticks {
        a.allow(identity(), POLICY, || Ok(Rc::new(DressingAccounting)))?;
    }
    Ok(())
}
fn snapshot(r: &SourceRoutes, v: &CallerView) -> owners::Result<Vec<Observation>> {
    let h = r.hero.query(frame::HERO, &[], &[], v)?;
    let (player, _, _) = frame::hero(&h)?;
    let a = r.actors.query(
        frame::ACTOR,
        &encode(&player, wand::MAX_REQUEST_BYTES)?,
        &[],
        v,
    )?;
    let e = r.equipment.query(frame::EQ, &[], &[], v)?;
    let Facts::Equipment(eq) = frame::fact(&e)? else {
        return Err(owners::Error::Schema);
    };
    let mut ids = eq.armor.iter().map(|(_, id)| *id).collect::<Vec<_>>();
    if let Some(d) = &eq.dressing {
        ids.push(d.item);
    }
    ids.sort();
    ids.dedup();
    if ids.len() > frame::MAX {
        return Err(owners::Error::Limit);
    }
    let o = r.objects.query(
        frame::ITEMS,
        &encode(&ids, wand::MAX_REQUEST_BYTES)?,
        &[],
        v,
    )?;
    Ok(vec![h, o, a, e])
}
pub(super) fn preview(r: &SourceRoutes, v: &CallerView) -> owners::Result<Vec<u8>> {
    let obs = snapshot(r, v)?;
    encode(
        &DressingRequest {
            dressing_i2d: true,
            guards: obs
                .iter()
                .map(|o| wand::OwnerWitness::from(o.stamp()))
                .collect(),
        },
        wand::MAX_RESPONSE_BYTES,
    )
}
pub(super) fn execute(
    r: &SourceRoutes,
    request: &[u8],
    v: &CallerView,
    c: &OwnerBridgeConfig,
) -> owners::Result<Vec<u8>> {
    let req: DressingRequest = decode(request, wand::MAX_REQUEST_BYTES)?;
    if !req.dressing_i2d {
        return Err(owners::Error::Unsupported);
    }
    let obs = snapshot(r, v)?;
    if req.guards
        != obs
            .iter()
            .map(|o| wand::OwnerWitness::from(o.stamp()))
            .collect::<Vec<_>>()
    {
        #[cfg(test)]
        STALE_REFUSALS.with(|x| x.set(x.get() + 1));
        return Err(owners::Error::Stale);
    }
    let Facts::Equipment(equipment) = frame::fact(&obs[3])? else {
        return Err(owners::Error::Schema);
    };
    let Facts::Actor(dead) = frame::fact(&obs[2])? else {
        return Err(owners::Error::Schema);
    };
    let local = encode(
        &LocalRequest {
            dressing_i2d: true,
            equipment,
            alive: {
                let alive = dead == Some(false);
                #[cfg(test)]
                {
                    if WRONG_ALIVE.with(|x| x.get()) {
                        !alive
                    } else {
                        alive
                    }
                }
                #[cfg(not(test))]
                {
                    alive
                }
            },
        },
        wand::MAX_REQUEST_BYTES,
    )?;
    let deps = vec![obs[0].clone(), obs[1].clone()];
    let e = r
        .equipment
        .prepare(frame::WRITE_EQ, &local, deps.clone(), v)?
        .erase();
    let a = r.actors.prepare(frame::WRITE_AC, &local, deps, v)?.erase();
    let ids = vec![r.equipment.id(), r.actors.id()];
    let batch = r.host.batch(
        vec![e, a],
        obs,
        &ids,
        ids.clone(),
        POLICY,
        Rc::new(DressingAccounting),
        profile_capability(c, frame::RUN),
        v,
    )?;
    let committed = batch.commit(v)?;
    #[cfg(test)]
    {
        let bytes = (r.equipment.inspect()?, r.actors.inspect()?);
        let history = r.host.history()?;
        assert_eq!(batch.commit(v), Err(owners::Error::Stale));
        assert_eq!(bytes, (r.equipment.inspect()?, r.actors.inspect()?));
        assert_eq!(history, r.host.history()?);
    }
    let events = committed
        .events
        .iter()
        .map(|e| decode::<crate::GameEvent>(e, wand::MAX_RESPONSE_BYTES))
        .collect::<owners::Result<Vec<_>>>()?;
    let error = if events.is_empty() {
        None
    } else {
        super::phase_support::publish_session(
            r,
            &committed,
            frame::RECORD,
            POLICY,
            profile_capability(c, frame::RUN),
            v,
        )
        .err()
        .map(|e| format!("{e:?}"))
    };
    encode(&DressingReply { events, error }, wand::MAX_RESPONSE_BYTES)
}
impl OwnerBridge {
    pub fn prepare_dressing_tick(&self) -> Result<DressingRequest, OwnerBridgeError> {
        if !self.config.dressing_ticks {
            return Err(owners::Error::Unsupported.into());
        }
        decode(
            &self.frame_call(
                frame::PREVIEW,
                &serde_json::json!({"dressing_i2d":true}),
                Scope::All,
            )?,
            wand::MAX_RESPONSE_BYTES,
        )
        .map_err(Into::into)
    }
    pub fn execute_dressing_tick(
        &self,
        request: DressingRequest,
    ) -> Result<DressingReply, OwnerBridgeError> {
        self.execute_dressing_tick_scoped(request, Scope::All)
    }
    pub fn execute_dressing_tick_scoped(
        &self,
        request: DressingRequest,
        scope: Scope,
    ) -> Result<DressingReply, OwnerBridgeError> {
        if !self.config.dressing_ticks {
            return Err(owners::Error::Unsupported.into());
        }
        decode(
            &self.frame_call(frame::RUN, &request, scope)?,
            wand::MAX_RESPONSE_BYTES,
        )
        .map_err(Into::into)
    }
    pub fn tick_dressing(&self) -> Result<DressingReply, OwnerBridgeError> {
        self.execute_dressing_tick(self.prepare_dressing_tick()?)
    }
}

//! Bounded phase metadata and private synchronous Session publication only.
use super::*;
pub(super) struct Metadata {
    pub prefix: &'static str,
    pub routes: &'static [(&'static str, &'static str, Access)],
    pub preview: &'static str,
    pub run: &'static str,
    pub dependent_write: &'static str,
    pub dependencies: &'static [(&'static str, &'static str, u8)],
}
impl Metadata {
    pub fn add_ports(
        &self,
        enabled: bool,
        name: &str,
        mut p: BTreeMap<String, Port>,
        ids: &BTreeMap<&str, owners::OwnerId>,
        c: &OwnerBridgeConfig,
    ) -> owners::Result<BTreeMap<String, Port>> {
        if enabled {
            for &(o, n, a) in self.routes {
                if o == name {
                    let dependencies = if n == self.dependent_write {
                        self.dependencies
                            .iter()
                            .map(|&(o, q, t)| {
                                Ok(Dependency {
                                    owner: *ids.get(o).ok_or(owners::Error::Schema)?,
                                    query: q.into(),
                                    schema: [c.schema_tag(t); 32],
                                })
                            })
                            .collect::<owners::Result<_>>()?
                    } else {
                        vec![]
                    };
                    p.insert(n.into(), port(a, dependencies));
                }
            }
        }
        Ok(p)
    }
    pub fn validate_ports(
        &self,
        enabled: bool,
        name: &str,
        p: &BTreeMap<String, Port>,
        c: &OwnerBridgeConfig,
    ) -> owners::Result<()> {
        let expected: Vec<_> = self
            .routes
            .iter()
            .filter(|(o, _, _)| *o == name && enabled)
            .collect();
        if p.keys().filter(|n| n.starts_with(self.prefix)).count() != expected.len() {
            return Err(owners::Error::Schema);
        }
        for (_, n, a) in expected {
            let x = p.get(*n).ok_or(owners::Error::Schema)?;
            let deps = if *n == self.dependent_write {
                self.dependencies
            } else {
                &[]
            };
            if x.major != 1
                || x.access != *a
                || x.dependencies.len() != deps.len()
                || x.dependencies
                    .iter()
                    .zip(deps)
                    .any(|(d, (_, q, t))| d.query != *q || d.schema != [c.schema_tag(*t); 32])
            {
                return Err(owners::Error::Schema);
            }
        }
        Ok(())
    }
    pub fn filtered_ports(&self, p: &BTreeMap<String, Port>) -> BTreeMap<String, Port> {
        p.iter()
            .filter(|(n, _)| !n.starts_with(self.prefix))
            .map(|(n, p)| (n.clone(), p.clone()))
            .collect()
    }
    fn endpoints(&self) -> impl Iterator<Item = (&'static str, Access)> + '_ {
        self.routes
            .iter()
            .map(|(_, n, a)| (*n, *a))
            .chain([(self.preview, Access::Read), (self.run, Access::Write)])
    }
    pub fn provider(&self, enabled: bool, mut m: Manifest) -> Manifest {
        if enabled {
            for (n, a) in self.endpoints() {
                m.exports.insert(n.into(), ep(a));
            }
        }
        m
    }
    pub fn caller(&self, enabled: bool, mut m: Manifest, c: &OwnerBridgeConfig) -> Manifest {
        if enabled {
            for (n, a) in self.endpoints() {
                m.dependencies.push(kernel::Dependency {
                    capability: profile_capability(c, n),
                    access: a,
                });
            }
            m.exports.insert(self.preview.into(), ep(Access::Read));
            m.exports.insert(self.run.into(), ep(Access::Write));
        }
        m
    }
    pub fn caller_ports(
        &self,
        enabled: bool,
        mut p: BTreeMap<String, NativePort>,
        c: &OwnerBridgeConfig,
    ) -> BTreeMap<String, NativePort> {
        if enabled {
            let m = profile_caller(c);
            for n in [self.preview, self.run] {
                let slot = m
                    .dependencies
                    .iter()
                    .position(|d| d.capability.endpoint == n)
                    .unwrap();
                p.insert(
                    n.into(),
                    Rc::new(move |req, v| v.call_dependency(slot, req)),
                );
            }
        }
        p
    }
    pub fn grant_names(&self, enabled: bool) -> BTreeSet<String> {
        if !enabled {
            return BTreeSet::new();
        }
        self.routes
            .iter()
            .map(|(o, n, _)| {
                format!(
                    "{}.{n}",
                    match *o {
                        "Hero" => 2,
                        "Actors" => 5,
                        "Session" => 3,
                        "Clock" => 8,
                        "Objects" => 4,
                        "Random" => 9,
                        _ => 0,
                    }
                )
            })
            .collect()
    }
    pub fn resources(&self, r: &SourceRoutes) -> owners::Result<Vec<Resource>> {
        self.routes
            .iter()
            .map(|(o, n, _)| match *o {
                "Hero" => r.hero.resource(n),
                "Actors" => r.actors.resource(n),
                "Session" => r.session.resource(n),
                "Clock" => r.clock.resource(n),
                "Objects" => r.objects.resource(n),
                "Random" => r.random.resource(n),
                _ => Err(owners::Error::Schema),
            })
            .collect()
    }
}
fn ep(a: Access) -> Endpoint {
    Endpoint {
        max_request: wand::MAX_REQUEST_BYTES as u32,
        max_reply: wand::MAX_RESPONSE_BYTES as u32,
        ..endpoint(a)
    }
}
pub(super) fn record(s: &mut SessionState, events: &[crate::GameEvent]) {
    s.event_log.extend_from_slice(events);
    let cap = s
        .max_event_log_len
        .unwrap_or(crate::SimulationWorld::DEFAULT_MAX_EVENT_LOG_LEN);
    if s.event_log.len() > cap {
        s.event_log.drain(0..s.event_log.len() - cap);
    }
}
pub(super) fn prepare_record(
    s: &SessionState,
    request: &[u8],
    obs: &[Observation],
) -> owners::Result<Proposal<SessionState>> {
    if !obs.is_empty() {
        return Err(owners::Error::Schema);
    }
    let events: Vec<crate::GameEvent> = decode(request, wand::MAX_REQUEST_BYTES)?;
    if events.len() > 5 {
        return Err(owners::Error::Limit);
    }
    let mut next = s.clone();
    record(&mut next, &events);
    Ok(Proposal {
        next,
        events: vec![],
        reply: b"null".to_vec(),
    })
}
// Per-call immutable evidence. No pending dynamic publication persistence.
struct SessionAccounting {
    command: &'static str,
    events: Vec<crate::GameEvent>,
}
impl Accounting for SessionAccounting {
    fn validate(&self, p: &[ProposalView], obs: &[Observation]) -> owners::Result<()> {
        if p.len() != 1
            || !obs.is_empty()
            || p[0].receipt.command != self.command
            || !p[0].events.is_empty()
            || p[0].reply != b"null"
        {
            return Err(owners::Error::Invariant);
        }
        let mut expected: SessionState = decode(&p[0].before, PAYLOAD)?;
        record(&mut expected, &self.events);
        if encode(&expected, PAYLOAD)? != p[0].proposed {
            return Err(owners::Error::Invariant);
        }
        Ok(())
    }
}
pub(super) fn publish_session(
    r: &SourceRoutes,
    committed: &owners::CommitReceipt,
    command: &'static str,
    policy: [u8; 32],
    cap: Capability,
    v: &CallerView,
) -> owners::Result<()> {
    let events = committed
        .events
        .iter()
        .map(|e| decode::<crate::GameEvent>(e, wand::MAX_RESPONSE_BYTES))
        .collect::<owners::Result<Vec<_>>>()?;
    let candidate = r
        .session
        .prepare(
            command,
            &encode(&events, wand::MAX_REQUEST_BYTES)?,
            vec![],
            v,
        )?
        .erase();
    let ids = vec![r.session.id()];
    let b = r.host.batch(
        vec![candidate],
        vec![],
        &ids,
        ids.clone(),
        policy,
        Rc::new(SessionAccounting { command, events }),
        cap,
        v,
    )?;
    b.commit(v)?;
    Ok(())
}

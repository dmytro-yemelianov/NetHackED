//! V2 native domain prerequisites. These ports do not dispatch gameplay.
//! Private owners prepare legal operations; the existing Host seals and publishes.
use super::*;
use wand::*;

mod charge;
mod domains;
mod rng;
pub use rng::{AuthenticatedWandRng, RawDrawError};

pub(super) const PROVIDER: &str = "native-owner-wand-i2b1-provider-v2";
pub(super) const CALLER: &str = "native-owner-wand-i2b1-caller-v2";
pub(super) const NATIVE_ID: &[u8] = b"NetHackED M5-I2B1 legal native owner operations v2";
const CALLER_ID: &[u8] = b"NetHackED M5-I2B1 authenticated domain invocation v2";
const RANDOM_ID: &[u8] = b"NetHackED M5-I2B1 authenticated raw random words v2";
const RANDOM: &str = "native-owner-wand-i2b1-random-v2";
const POLICY: [u8; 32] = [227; 32];
const OPERATE: &str = "operate_v2";
const COMMANDS: [(&str, &str); 7] = [
    ("Objects", "wand_objects_command_v2"),
    ("Actors", "wand_actors_command_v2"),
    ("Geography", "wand_geography_command_v2"),
    ("Hero", "wand_hero_command_v2"),
    ("Clock", "wand_clock_command_v2"),
    ("Random", "wand_random_command_v2"),
    ("Clock", "wand_clock_energy_v2"),
];
const READS: [(&str, &str); 6] = [
    ("Objects", "wand_objects_facts_v2"),
    ("Actors", "wand_actors_facts_v2"),
    ("Geography", "wand_geography_facts_v2"),
    ("Hero", "wand_hero_facts_v2"),
    ("Session", "wand_session_facts_v2"),
    ("Clock", "wand_clock_admission_v2"),
];
impl CommandQuery {
    pub(crate) fn route(&self) -> (&'static str, &'static str) {
        READS[match self {
            Self::WrestPreview => return ("Random", "wand_random"),
            Self::LifetimeObject { .. } => return ("Objects", super::item_lifetime::OBJECT_QUERY),
            Self::LifetimeBilling { .. } => return ("Billing", super::item_lifetime::BILL_QUERY),
            Self::Object { .. } => 0,
            Self::Actor { .. } => 1,
            Self::Geography { .. } => 2,
            Self::Hero | Self::Genocide { .. } => 3,
            Self::Session => 4,
            Self::TimerAdmission { .. } => 5,
            Self::Gameplay(q) => return q.route(),
        }]
    }
}
impl WandOwnerCommand {
    pub(crate) fn route(&self) -> (&'static str, &'static str) {
        COMMANDS[match self {
            Self::Objects(_) => 0,
            Self::Actors(_) => 1,
            Self::Geography { .. } => 2,
            Self::Hero(_) => 3,
            Self::Clock(ClockOperation::HeroAct) => 6,
            Self::Clock(_) => 4,
            Self::Random { .. } | Self::Sample { .. } => 5,
        }]
    }
}
pub(super) fn read_routes(c: &OwnerBridgeConfig) -> Vec<(&'static str, &'static str)> {
    let mut routes = WAND_ROUTES.to_vec();
    if c.wand_commands {
        routes.extend(READS);
        if c.wand_gameplay {
            routes.extend(super::wand_gameplay::READS);
        }
        if c.item_lifetime {
            routes.extend(super::item_lifetime::READS);
        }
    }
    routes
}
pub(super) fn random_id(c: &OwnerBridgeConfig) -> &'static [u8] {
    if c.regeneration_ticks {
        return b"NetHackED regeneration random_id M6-I2c v1";
    }
    if c.metabolic_ticks {
        return b"NetHackED metabolic random_id M6-I2b v1";
    }
    if c.passive_ticks {
        return b"NetHackED passive random M6-I2a v1";
    }
    if c.default_commands {
        return b"NetHackED default frame random identity M6-I1 v1";
    }
    if c.wand_charge_attempts {
        return b"NetHackED atomic charge random_id i5c v1";
    }
    if c.wand_workflow_replies {
        b"NetHackED detailed workflow random owner adapter i5 v1"
    } else if c.wand_workflows {
        b"NetHackED workflow random owner adapter i4 v2"
    } else if c.wand_gameplay {
        super::wand_gameplay::RANDOM_ID
    } else if c.wand_commands {
        RANDOM_ID
    } else {
        I1_RANDOM_ID
    }
}
pub(super) fn caller_id(c: &OwnerBridgeConfig) -> &'static [u8] {
    if c.dressing_ticks {
        return b"NetHackED dressing caller M6-I2d v1";
    }
    if c.regeneration_ticks {
        return b"NetHackED regeneration caller_id M6-I2c v1";
    }
    if c.metabolic_ticks {
        return b"NetHackED metabolic caller_id M6-I2b v1";
    }
    if c.passive_ticks {
        return b"NetHackED passive caller M6-I2a v1";
    }
    if c.default_commands {
        return b"NetHackED default frame caller identity M6-I1 v1";
    }
    if c.wand_charge_attempts {
        return b"NetHackED atomic charge caller_id i5c v1";
    }
    if c.wand_workflow_replies {
        b"NetHackED detailed workflow-capable wand caller i5 v1"
    } else if c.wand_workflows {
        b"NetHackED workflow-capable wand caller i4 v2"
    } else if c.wand_extensions {
        super::wand_extensions::CALLER_ID
    } else if c.item_lifetime {
        super::item_lifetime::CALLER_ID
    } else if c.wand_gameplay {
        super::wand_gameplay::CALLER_ID
    } else if c.wand_commands {
        CALLER_ID
    } else {
        I1_CALLER_ID
    }
}
pub(super) fn random_capability(c: &OwnerBridgeConfig) -> Capability {
    if c.wand_charge_attempts || c.regeneration_ticks {
        return profile_capability(c, "wand_random");
    }
    if !c.wand_commands {
        return super::random_capability();
    }
    Capability {
        provider: if c.wand_gameplay {
            super::wand_gameplay::RANDOM
        } else {
            RANDOM
        }
        .into(),
        endpoint: "wand_random".into(),
        major: 1,
    }
}
fn command_capability(c: &OwnerBridgeConfig, endpoint: &str) -> Capability {
    if c.wand_charge_attempts || c.regeneration_ticks {
        return profile_capability(c, endpoint);
    }
    if endpoint == COMMANDS[5].1 {
        Capability {
            provider: if c.wand_gameplay {
                super::wand_gameplay::RANDOM
            } else {
                RANDOM
            }
            .into(),
            endpoint: endpoint.into(),
            major: 1,
        }
    } else {
        profile_capability(c, endpoint)
    }
}
pub(super) fn random_manifest(c: &OwnerBridgeConfig) -> Manifest {
    let mut m = super::random_manifest();
    if c.wand_commands {
        m.id = if c.wand_gameplay {
            super::wand_gameplay::RANDOM
        } else {
            RANDOM
        }
        .into();
        m.exports.insert(COMMANDS[5].1.into(), command_endpoint());
    }
    m
}
fn command_endpoint() -> Endpoint {
    Endpoint {
        max_request: MAX_REQUEST_BYTES as u32,
        max_reply: MAX_RESPONSE_BYTES as u32,
        ..endpoint(Access::Write)
    }
}
pub(super) fn provider_manifest(mut m: Manifest, c: &OwnerBridgeConfig) -> Manifest {
    if c.wand_commands {
        for (_, name) in COMMANDS.iter().filter(|r| r.0 != "Random") {
            m.exports.insert((*name).into(), command_endpoint());
        }
    }
    if c.wand_charge_attempts || c.regeneration_ticks {
        m.exports
            .insert("wand_random".into(), endpoint(Access::Read));
        m.exports.insert(COMMANDS[5].1.into(), command_endpoint());
    }
    if c.item_lifetime {
        m.exports
            .insert(super::item_lifetime::COMMAND.into(), command_endpoint());
    }
    m
}
pub(super) fn caller_manifest(mut m: Manifest, c: &OwnerBridgeConfig) -> Manifest {
    if c.wand_commands {
        for (_, name) in COMMANDS {
            m.dependencies.push(kernel::Dependency {
                capability: command_capability(c, name),
                access: Access::Write,
            });
        }
        m.exports.insert(OPERATE.into(), command_endpoint());
        if c.wand_gameplay {
            m.exports.insert(
                super::wand_gameplay::ENDPOINT.into(),
                super::wand_gameplay::endpoint(),
            );
        }
        if c.item_lifetime {
            m.dependencies.push(kernel::Dependency {
                capability: profile_capability(c, super::item_lifetime::COMMAND),
                access: Access::Write,
            });
            m.exports
                .insert(super::item_lifetime::ENDPOINT.into(), command_endpoint());
        }
    }
    if c.wand_extensions {
        m.dependencies.push(kernel::Dependency {
            capability: Capability {
                provider: super::wand_extensions::POLICY_PROVIDER.into(),
                endpoint: super::wand_extensions::POLICY_ENDPOINT.into(),
                major: 1,
            },
            access: Access::Read,
        });
    }
    m
}
pub(super) fn command_count(name: &str, c: &OwnerBridgeConfig) -> usize {
    if c.wand_commands {
        COMMANDS.iter().filter(|r| r.0 == name).count()
            + usize::from(c.item_lifetime && super::item_lifetime::LIFETIME_OWNERS.contains(&name))
    } else {
        0
    }
}
fn declarations(
    name: &str,
    command: &str,
    c: &OwnerBridgeConfig,
) -> Vec<(&'static str, &'static str, u8)> {
    if c.wand_charge_attempts && name == "Objects" {
        let mut d = wand_declarations("Objects");
        d.push(("Random", "wand_random", 108));
        return d;
    }
    if command == "wand_clock_energy_v2" {
        return vec![];
    }
    match name {
        "Objects" => wand_declarations("Objects"),
        "Actors" => vec![
            ("Geography", "wand_geography", 101),
            ("Hero", "wand_hero_facts_v2", 104),
        ],
        // Corpse timers declare bounded object facts. HeroAct has its own
        // endpoint with no cross-owner dependency.
        "Clock" => vec![("Objects", "wand_objects_facts_v2", 102)],
        _ => vec![],
    }
}
pub(super) fn add_ports(
    name: &str,
    mut ports: BTreeMap<String, Port>,
    ids: &BTreeMap<&str, owners::OwnerId>,
    c: &OwnerBridgeConfig,
) -> owners::Result<BTreeMap<String, Port>> {
    if c.wand_commands {
        for (_, command) in COMMANDS.iter().filter(|r| r.0 == name) {
            let dependencies = declarations(name, command, c)
                .into_iter()
                .map(|(owner, query, tag)| {
                    Ok(Dependency {
                        owner: *ids.get(owner).ok_or(owners::Error::Schema)?,
                        query: query.into(),
                        schema: [c.schema_tag(tag - 100); 32],
                    })
                })
                .collect::<owners::Result<_>>()?;
            ports.insert((*command).into(), port(Access::Write, dependencies));
        }
    }
    super::item_lifetime::add_ports(name, ports, ids, c)
}
pub(super) fn validate_ports(
    name: &str,
    ports: &BTreeMap<String, Port>,
    c: &OwnerBridgeConfig,
) -> owners::Result<()> {
    if !c.wand_commands {
        return Ok(());
    }
    for (_, command) in COMMANDS.iter().filter(|r| r.0 == name) {
        let p = ports.get(*command).ok_or(owners::Error::Schema)?;
        let deps = declarations(name, command, c);
        if p.major != 1
            || p.access != Access::Write
            || p.dependencies.len() != deps.len()
            || p.dependencies.iter().zip(deps).any(|(d, (_, query, tag))| {
                d.query != query || d.schema != [c.schema_tag(tag - 100); 32]
            })
        {
            return Err(owners::Error::Schema);
        }
    }
    super::item_lifetime::validate_ports(name, ports, c)
}
pub(super) fn command_index(c: &WandOwnerCommand) -> Option<usize> {
    COMMANDS.iter().position(|r| *r == c.route())
}
pub(super) fn query_dependencies(q: &CommandQuery) -> Vec<WandQuery> {
    match q {
        CommandQuery::Gameplay(super::wand_gameplay::FactQuery::Genocide { version, offset }) => {
            vec![WandQuery::Command(CommandQuery::Gameplay(
                super::wand_gameplay::FactQuery::GenocideRegistry {
                    version: *version,
                    offset: *offset,
                },
            ))]
        }
        CommandQuery::Gameplay(super::wand_gameplay::FactQuery::Objects(_)) => vec![
            WandQuery::Hero,
            WandQuery::Session,
            geo(Residency::Active, vec![]),
        ],
        CommandQuery::Gameplay(super::wand_gameplay::FactQuery::Actors(_)) => {
            vec![geo(Residency::Active, vec![])]
        }
        CommandQuery::Object { target } | CommandQuery::LifetimeObject { target } => vec![
            WandQuery::Hero,
            WandQuery::Session,
            geo(target.residency, vec![]),
        ],
        CommandQuery::LifetimeBilling { target } => vec![
            WandQuery::Command(CommandQuery::LifetimeObject { target: *target }),
            geo(target.residency, vec![]),
        ],
        CommandQuery::Actor { .. } => vec![geo(Residency::Active, vec![])],
        CommandQuery::TimerAdmission { target, .. } => {
            vec![WandQuery::Objects(ObjectRequest::Identity(*target))]
        }
        _ => vec![],
    }
}
fn geo(residency: Residency, cells: Vec<nethacked_types::Coord>) -> WandQuery {
    WandQuery::Geography(GeographyRequest {
        residency,
        cells,
        trajectory: None,
    })
}
pub(super) fn query_owner(
    r: &SourceRoutes,
    q: &CommandQuery,
    name: &str,
    request: &[u8],
    obs: &[Observation],
    view: &CallerView,
) -> owners::Result<Observation> {
    match q {
        CommandQuery::Gameplay(super::wand_gameplay::FactQuery::Objects(_))
        | CommandQuery::Object { .. }
        | CommandQuery::LifetimeObject { .. } => r.objects.query(name, request, obs, view),
        CommandQuery::LifetimeBilling { .. } => r.billing.query(name, request, obs, view),
        CommandQuery::Gameplay(super::wand_gameplay::FactQuery::Actors(_))
        | CommandQuery::Actor { .. } => r.actors.query(name, request, obs, view),
        CommandQuery::Gameplay(super::wand_gameplay::FactQuery::Genocide { .. }) => {
            r.actors.query(name, request, obs, view)
        }
        CommandQuery::Geography { .. } => r.geography.query(name, request, obs, view),
        CommandQuery::Hero
        | CommandQuery::Genocide { .. }
        | CommandQuery::Gameplay(super::wand_gameplay::FactQuery::GenocideRegistry { .. }) => {
            r.hero.query(name, request, obs, view)
        }
        CommandQuery::Session => r.session.query(name, request, obs, view),
        CommandQuery::WrestPreview => r.random.query(name, request, obs, view),
        CommandQuery::Gameplay(super::wand_gameplay::FactQuery::Clock)
        | CommandQuery::TimerAdmission { .. } => r.clock.query(name, request, obs, view),
    }
}
pub(super) fn dependencies(command: &WandOwnerCommand, c: &OwnerBridgeConfig) -> Vec<WandQuery> {
    match command {
        WandOwnerCommand::Objects(op) => {
            let (residency, cells) = match op {
                ObjectOperation::ChargeAttempt { target, .. }
                | ObjectOperation::PositiveCharge { target, .. } => (target.residency, vec![]),
                ObjectOperation::PublishGenerated { object } => (
                    Residency::Active,
                    match object.location {
                        nethacked_arena::ItemLocation::Floor(c) => vec![c],
                        _ => vec![],
                    },
                ),
            };
            let mut q = vec![WandQuery::Hero, WandQuery::Session, geo(residency, cells)];
            if c.wand_charge_attempts {
                q.push(
                    if matches!(op, ObjectOperation::ChargeAttempt { expected: 0, .. }) {
                        WandQuery::Command(CommandQuery::WrestPreview)
                    } else {
                        WandQuery::Random { draws: 0 }
                    },
                );
            }
            q
        }
        WandOwnerCommand::Actors(op) => vec![
            geo(
                Residency::Active,
                match op {
                    ActorOperation::PublishInitialized { actor } => vec![actor.coord],
                    _ => vec![],
                },
            ),
            WandQuery::Command(match op {
                ActorOperation::PublishInitialized { actor } => CommandQuery::Genocide {
                    definition: actor.name.clone(),
                },
                _ => CommandQuery::Hero,
            }),
        ],
        WandOwnerCommand::Clock(ClockOperation::HeroAct) => vec![],
        WandOwnerCommand::Clock(
            ClockOperation::StartCorpseTimer { target, .. }
            | ClockOperation::CancelCorpseTimers { target },
        ) => vec![WandQuery::Command(CommandQuery::Object { target: *target })],
        _ => vec![],
    }
}
fn witnesses(obs: &[Observation]) -> Vec<OwnerWitness> {
    obs.iter().map(|o| OwnerWitness::from(o.stamp())).collect()
}
fn gather(
    r: &SourceRoutes,
    command: &WandOwnerCommand,
    view: &CallerView,
    c: &OwnerBridgeConfig,
) -> owners::Result<(Vec<Observation>, Vec<Observation>)> {
    let mut immediate = vec![];
    if let WandOwnerCommand::Objects(ObjectOperation::ChargeAttempt { target, expected }) = command
    {
        charge::preflight(r, *target, *expected, view, c)?;
    }
    let mut provenance = vec![];
    for q in dependencies(command, c) {
        let (o, nested) = r.observe(&q, view)?;
        provenance.extend(nested);
        immediate.push(o.clone());
        provenance.push(o);
    }
    Ok((immediate, provenance))
}

pub(crate) trait CommandDomain: Clone + super::item_lifetime::LifetimeDomain {
    fn command_facts(
        &self,
        _: &CommandQuery,
        _: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<CommandFacts> {
        Err(owners::Error::Unsupported)
    }
    fn apply(
        &mut self,
        _: &WandOwnerCommand,
        _: &[Observation],
        _: &OwnerBridgeConfig,
    ) -> owners::Result<OperationResult> {
        Err(owners::Error::Unsupported)
    }
    fn lifetime_prepare(
        &self,
        request: &super::item_lifetime::Retirement,
        obs: &[Observation],
        c: &OwnerBridgeConfig,
    ) -> owners::Result<Proposal<Self>> {
        self.prepare_lifetime(request, obs, c)
    }
}
pub(super) fn prepare<T: CommandDomain>(
    s: &T,
    name: &str,
    bytes: &[u8],
    obs: &[Observation],
    c: &OwnerBridgeConfig,
) -> owners::Result<Proposal<T>> {
    if !c.wand_commands {
        return Err(owners::Error::Unsupported);
    }
    if name == super::item_lifetime::COMMAND {
        if !c.item_lifetime {
            return Err(owners::Error::Unsupported);
        }
        let request = decode(bytes, MAX_REQUEST_BYTES)?;
        return s.lifetime_prepare(&request, obs, c);
    }
    let request: WandOperation = decode(bytes, MAX_REQUEST_BYTES)?;
    if request.command.route().1 != name {
        return Err(owners::Error::Schema);
    }
    let mut next = s.clone();
    let result = next.apply(&request.command, obs, c)?;
    Ok(Proposal {
        next,
        events: vec![],
        reply: encode(&result, MAX_RESPONSE_BYTES)?,
    })
}
struct CommandAccounting;
impl Accounting for CommandAccounting {
    fn validate(&self, p: &[ProposalView], _: &[Observation]) -> owners::Result<()> {
        if p.len() != 1
            || !COMMANDS
                .iter()
                .any(|(_, name)| p[0].receipt.command == *name)
            || !p[0].events.is_empty()
            || p[0].receipt.stamp.schema[0] < 201
        {
            return Err(owners::Error::Invariant);
        }
        let _: OperationResult = decode(&p[0].reply, MAX_RESPONSE_BYTES)?;
        Ok(())
    }
}
fn accounting_identity() -> op::ArtifactIdentity {
    artifact("native-wand-i2b1-command-accounting", 227)
}
pub(super) fn capture_accounting(
    a: &mut op::AccountingCaptureCatalog,
    c: &OwnerBridgeConfig,
) -> owners::Result<()> {
    if c.wand_commands {
        a.register(accounting_identity(), POLICY, Rc::new(CommandAccounting))?;
    }
    charge::capture(a, c)?;
    super::default_commands::capture(a, c)?;
    super::dressing_ticks::capture(a, c)?;
    super::passive_ticks::capture(a, c)?;
    super::metabolic_ticks::capture(a, c)?;
    super::regeneration_ticks::capture(a, c)?;
    super::timer_ticks::capture(a, c)?;
    super::item_lifetime::capture_accounting(a, c)?;
    Ok(())
}
pub(super) fn restore_accounting(
    a: &mut op::AccountingRestoreCatalog,
    c: &OwnerBridgeConfig,
) -> owners::Result<()> {
    if c.wand_commands {
        a.allow(accounting_identity(), POLICY, || {
            Ok(Rc::new(CommandAccounting))
        })?;
    }
    charge::restore(a, c)?;
    super::default_commands::restore(a, c)?;
    super::dressing_ticks::restore(a, c)?;
    super::passive_ticks::restore(a, c)?;
    super::metabolic_ticks::restore(a, c)?;
    super::regeneration_ticks::restore(a, c)?;
    super::timer_ticks::restore(a, c)?;
    super::item_lifetime::restore_accounting(a, c)?;
    Ok(())
}
fn execute(
    r: &SourceRoutes,
    endpoint: &str,
    bytes: &[u8],
    view: &CallerView,
    c: &OwnerBridgeConfig,
) -> owners::Result<Vec<u8>> {
    let operation = view.operation().ok_or(owners::Error::Authority)?;
    if operation.phase() != endpoint
        && !(c.wand_gameplay && operation.phase() == super::wand_gameplay::ENDPOINT)
        && !super::wand_workflows::allows_gameplay_owner(c, view, operation.phase())
    {
        return Err(owners::Error::Authority);
    }
    let request: WandOperation = decode(bytes, MAX_REQUEST_BYTES)?;
    if request.command.route().1 != endpoint || request.dependencies.len() > MAX_REFERENCES {
        return Err(owners::Error::Schema);
    }
    let (owner, name) = request.command.route();
    if OwnerWitness::from(&r.stamp(owner)?) != request.expected_owner {
        return Err(owners::Error::Stale);
    }
    let (immediate, provenance) = gather(r, &request.command, view, c)?;
    if witnesses(&provenance) != request.dependencies {
        return Err(owners::Error::Stale);
    }
    if let WandOwnerCommand::Objects(ObjectOperation::ChargeAttempt { target, expected }) =
        request.command
    {
        return charge::execute(r, target, expected, immediate, provenance, view, c);
    }
    if let WandOwnerCommand::Clock(ClockOperation::StartCorpseTimer { target, kind, due }) =
        &request.command
    {
        let query = WandQuery::Command(CommandQuery::TimerAdmission {
            target: *target,
            kind: *kind,
            due: *due,
        });
        let (admission, _) = r.observe(&query, view)?;
        let fact: WandFacts = decode(admission.bytes(), MAX_RESPONSE_BYTES)?;
        if let WandFacts::Command(CommandFacts::TimerAdmission {
            result: Err(reason),
        }) = fact
        {
            return encode(
                &OperationResult::TimerRefused { reason },
                MAX_RESPONSE_BYTES,
            );
        }
    }
    macro_rules! stage {
        ($owner:ident) => {
            r.$owner.prepare(name, bytes, immediate, view)?.erase()
        };
    }
    let candidate = match &request.command {
        WandOwnerCommand::Objects(_) => stage!(objects),
        WandOwnerCommand::Actors(_) => stage!(actors),
        WandOwnerCommand::Geography { .. } => stage!(geography),
        WandOwnerCommand::Hero(_) => stage!(hero),
        WandOwnerCommand::Clock(_) => stage!(clock),
        WandOwnerCommand::Random { .. } | WandOwnerCommand::Sample { .. } => stage!(random),
    };
    let id = r.stamp(owner)?.owner;
    let batch = r.host.batch(
        vec![candidate],
        provenance,
        &[id],
        vec![id],
        POLICY,
        Rc::new(CommandAccounting),
        command_capability(c, name),
        view,
    )?;
    let receipt = match batch.commit(view) {
        Ok(receipt) => receipt,
        Err(e) => {
            batch.discard()?;
            return Err(e);
        }
    };
    // Existing publication encoding: one u64 little-endian reply length.
    let len = receipt.reply.get(16..24).ok_or(owners::Error::Invariant)?;
    let len = u64::from_le_bytes(len.try_into().map_err(|_| owners::Error::Invariant)?) as usize;
    if receipt.reply.len() != len + 24 {
        return Err(owners::Error::Invariant);
    }
    Ok(receipt.reply[24..].to_vec())
}
fn command_port(routes: Rc<Routes>, c: OwnerBridgeConfig, endpoint: &'static str) -> NativePort {
    Rc::new(move |request, view| {
        execute(&routes.get()?, endpoint, request, view, &c).map_err(native_refusal)
    })
}
pub(super) fn native_ports(
    mut ports: BTreeMap<String, NativePort>,
    routes: Rc<Routes>,
    c: OwnerBridgeConfig,
) -> BTreeMap<String, NativePort> {
    if c.wand_commands {
        for (_, name) in COMMANDS.iter().filter(|r| r.0 != "Random") {
            ports.insert(
                (*name).into(),
                command_port(routes.clone(), c.clone(), name),
            );
        }
    }
    super::item_lifetime::native_ports(ports, routes, c)
}
pub(super) fn random_ports(
    mut ports: BTreeMap<String, NativePort>,
    routes: Rc<Routes>,
    c: OwnerBridgeConfig,
) -> BTreeMap<String, NativePort> {
    if c.wand_commands {
        ports.insert(COMMANDS[5].1.into(), command_port(routes, c, COMMANDS[5].1));
    }
    ports
}
pub(super) fn caller_ports(
    mut ports: BTreeMap<String, NativePort>,
    c: &OwnerBridgeConfig,
) -> BTreeMap<String, NativePort> {
    if c.wand_commands {
        let base = 3 + read_routes(c).len();
        ports.insert(
            OPERATE.into(),
            Rc::new(move |request, view| {
                let q: WandOperation =
                    decode(request, MAX_REQUEST_BYTES).map_err(native_refusal)?;
                let index = COMMANDS
                    .iter()
                    .position(|r| *r == q.command.route())
                    .ok_or(kernel::Error::MissingDependency)?;
                view.call_dependency(base + index, request)
            }),
        );
    }
    super::item_lifetime::caller_ports(super::wand_gameplay::caller_ports(ports, c), c)
}

#[allow(clippy::too_many_arguments)]
pub(super) fn resources(
    mut keys: BTreeSet<Resource>,
    host: &Host,
    geography: &Rc<owners::Owner<GeographyState>>,
    objects: &Rc<owners::Owner<ObjectsState>>,
    actors: &Rc<owners::Owner<ActorsState>>,
    hero: &Rc<owners::Owner<HeroState>>,
    clock: &Rc<owners::Owner<ClockState>>,
    random: &Rc<owners::Owner<RandomState>>,
    c: &OwnerBridgeConfig,
) -> owners::Result<BTreeSet<Resource>> {
    if c.wand_commands {
        for (owner, name) in COMMANDS {
            keys.insert(match owner {
                "Objects" => objects.resource(name)?,
                "Actors" => actors.resource(name)?,
                "Geography" => geography.resource(name)?,
                "Hero" => hero.resource(name)?,
                "Clock" => clock.resource(name)?,
                "Random" => random.resource(name)?,
                _ => return Err(owners::Error::Schema),
            });
        }
        keys.insert(host.publication_resource());
    }
    Ok(keys)
}
pub(super) fn random_resources(
    random: &Rc<owners::Owner<RandomState>>,
    host: &Host,
    c: &OwnerBridgeConfig,
) -> owners::Result<BTreeSet<Resource>> {
    let mut keys: BTreeSet<_> = [random.resource("wand_random")?].into();
    if c.wand_commands {
        keys.insert(random.resource(COMMANDS[5].1)?);
        keys.insert(host.publication_resource());
    }
    Ok(keys)
}
pub(super) fn charge_grant_names(
    names: BTreeSet<String>,
    c: &OwnerBridgeConfig,
) -> BTreeSet<String> {
    if !c.wand_charge_attempts {
        return names;
    }
    names
        .into_iter()
        .map(|key| {
            let Some((number, tail)) = key.split_once('.') else {
                return key;
            };
            let number = match number {
                "4" => 5,
                "5" => 6,
                "6" => 7,
                "7" => 8,
                "8" => 9,
                "9" => 4,
                _ => return key,
            };
            format!("{number}.{tail}")
        })
        .collect()
}
pub(super) fn gameplay_grant_names(c: &OwnerBridgeConfig) -> BTreeSet<String> {
    let mut expected: BTreeSet<String> = [
        "publication",
        "2.query",
        "3.query",
        "4.query",
        "7.admit",
        "7.record_bill_terms",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    let number = |owner: &str| match owner {
        "Geography" => 1,
        "Hero" => 2,
        "Session" => 3,
        "Objects" => 4,
        "Actors" => 5,
        "Equipment" => 6,
        "Billing" => 7,
        "Clock" => 8,
        "Random" => 9,
        _ => 0,
    };
    for (owner, name) in read_routes(c).into_iter().chain(COMMANDS) {
        expected.insert(format!("{}.{name}", number(owner)));
    }
    if c.item_lifetime {
        for owner in super::item_lifetime::LIFETIME_OWNERS {
            expected.insert(format!(
                "{}.{}",
                number(owner),
                super::item_lifetime::COMMAND
            ));
        }
    }
    expected.extend(super::default_commands::grant_names(c));
    expected.extend(super::passive_ticks::grant_names(c));
    expected.extend(super::metabolic_ticks::grant_names(c));
    expected.extend(super::regeneration_ticks::grant_names(c));
    charge_grant_names(expected, c)
}
pub(super) fn approve_grant(
    caller: &kp::SavedModule,
    slot: usize,
    target: &kp::SavedModule,
    requested: &Authority,
    c: &OwnerBridgeConfig,
) -> kernel::Result<Authority> {
    let manifest = profile_caller(c);
    let dep = manifest
        .dependencies
        .get(slot)
        .ok_or(kernel::Error::Authority)?;
    if c.wand_extensions && dep.capability.provider == super::wand_extensions::POLICY_PROVIDER {
        if caller.manifest == manifest
            && target.manifest == super::wand_extensions::policy_manifest()
            && requested
                == &(Authority {
                    access: Access::Read,
                    scope: Scope::Keys(BTreeSet::new()),
                })
        {
            return Ok(requested.clone());
        }
        return Err(kernel::Error::Authority);
    }
    let random = !c.wand_charge_attempts
        && !c.regeneration_ticks
        && dep.capability.provider == random_capability(c).provider;
    if caller.manifest != manifest
        || target.manifest
            != if random {
                random_manifest(c)
            } else {
                profile_provider(c)
            }
        || requested.access != dep.access
    {
        return Err(kernel::Error::Authority);
    }
    let Scope::Keys(keys) = &requested.scope else {
        return Err(kernel::Error::Authority);
    };
    let namespace = &keys.first().ok_or(kernel::Error::Authority)?.owner;
    let mut expected = gameplay_grant_names(c);
    if c.item_lifetime && dep.capability.endpoint == super::item_lifetime::COMMAND {
        expected = charge_grant_names(super::item_lifetime::grant_names(), c);
    }
    if random {
        expected = charge_grant_names(
            [
                "publication".into(),
                "9.wand_random".into(),
                format!("9.{}", COMMANDS[5].1),
            ]
            .into(),
            c,
        );
    }
    if namespace
        .strip_prefix("owners-")
        .is_none_or(|s| s.parse::<u64>().is_err())
        || keys.iter().any(|r| r.owner != *namespace)
        || keys.iter().map(|r| r.key.clone()).collect::<BTreeSet<_>>() != expected
    {
        return Err(kernel::Error::Authority);
    }
    Ok(requested.clone())
}
impl OwnerBridge {
    /// Capture finite compare guards through authenticated read invocations.
    /// Preparing this request does not allocate a ticket, change state or draw.
    pub fn wand_operation(
        &self,
        command: WandOwnerCommand,
    ) -> Result<WandOperation, OwnerBridgeError> {
        if !self.config.wand_commands {
            return Err(owners::Error::Unsupported.into());
        }
        let expected_owner = self.session.with_runtime(&self.credential.borrow(), |b| {
            macro_rules! stamp {
                ($state:ident, $name:literal) => {
                    b.owner::<$state>($name)
                        .ok_or(owners::Error::Schema)?
                        .owner
                        .stamp()
                };
            }
            let stamp = match command.route().0 {
                "Objects" => stamp!(ObjectsState, "Objects"),
                "Actors" => stamp!(ActorsState, "Actors"),
                "Geography" => stamp!(GeographyState, "Geography"),
                "Hero" => stamp!(HeroState, "Hero"),
                "Clock" => stamp!(ClockState, "Clock"),
                "Random" => stamp!(RandomState, "Random"),
                _ => Err(owners::Error::Schema),
            }?;
            Ok::<_, owners::Error>(OwnerWitness::from(&stamp))
        })??;
        if let WandOwnerCommand::Objects(ObjectOperation::ChargeAttempt { target, expected }) =
            &command
        {
            charge::guard_bridge(self, *target, *expected)?;
        }
        let mut guards = vec![];
        for query in dependencies(&command, &self.config) {
            let o = self.observe_wand(WandRead {
                query,
                expected_owner: None,
            })?;
            guards.extend(o.dependencies.iter().map(OwnerWitness::from));
            guards.push(o.witness());
        }
        Ok(WandOperation {
            expected_owner,
            dependencies: guards,
            command,
        })
    }
    /// Execute one legal domain operation through the V2 native caller. Actual
    /// selected-wand gameplay dispatch belongs to the subsequent integration.
    pub fn operate_wand(
        &self,
        request: WandOperation,
    ) -> Result<OperationResult, OwnerBridgeError> {
        self.operate_wand_scoped(request, Scope::All)
    }
    pub fn operate_wand_scoped(
        &self,
        request: WandOperation,
        scope: Scope,
    ) -> Result<OperationResult, OwnerBridgeError> {
        if !self.config.wand_commands {
            return Err(owners::Error::Unsupported.into());
        }
        let bytes = encode(&request, MAX_REQUEST_BYTES)?;
        let response = self.session.with_runtime(&self.credential.borrow(), |b| {
            let caller = bridge_caller(b, &self.config)?;
            let cleanup = b.host().cleanup_identity()?;
            let (lease, operation) = b.update_kernel(|k| {
                let lease = k.capture_pipeline(
                    caller,
                    OPERATE,
                    Authority {
                        access: Access::Write,
                        scope: scope.clone(),
                    },
                )?;
                let id = k.issue_operation()?;
                let operation = k.operation_binding_with_cleanup(
                    id,
                    lease,
                    1,
                    request.command.route().1,
                    &[cleanup],
                )?;
                Ok::<_, kernel::Error>((lease, operation))
            })??;
            let result = b.kernel()?.invoke_operation(
                &operation,
                &bytes,
                Authority {
                    access: Access::Write,
                    scope,
                },
            );
            b.update_kernel(|k| k.release_pipeline(lease))??;
            Ok::<_, OwnerBridgeError>(decode(&result?, MAX_RESPONSE_BYTES)?)
        })?;
        self.session
            .replace_configuration(&self.key, restore_configuration(&self.config)?)?;
        self.credential
            .replace(self.session.issue_credential(&self.key)?);
        response
    }
}

/// Command-profile admission precedes executable factories. Hash the kernel's
/// canonical native-v1 identity framing; no owner payload or authority is built.
pub(super) fn admit_profile(
    registry: &kp::RegistrySnapshot,
    c: &OwnerBridgeConfig,
) -> owners::Result<()> {
    if !c.wand_commands {
        return Ok(());
    }
    use sha2::{Digest, Sha256};
    let mut expected = vec![
        (profile_provider(c), profile_native_id(c)),
        (random_manifest(c), random_id(c)),
        (profile_caller(c), caller_id(c)),
    ];
    if c.wand_extensions {
        expected.push((
            super::wand_extensions::policy_manifest(),
            super::wand_extensions::POLICY_ID,
        ));
    }
    if c.wand_workflows {
        expected.push((
            super::wand_workflows::manifest(c),
            super::wand_workflows::code(c),
        ));
    }
    let guests: Vec<_> = registry
        .modules
        .iter()
        .filter(|m| m.executable == kp::Executable::WasmAbiV1)
        .collect();
    if registry.modules.len() != expected.len() + guests.len()
        || (!c.wand_extensions && !guests.is_empty())
    {
        return Err(owners::Error::Schema);
    }
    for guest in guests {
        if !c
            .extension_catalog
            .iter()
            .any(|g| g.manifest() == &guest.manifest)
        {
            return Err(owners::Error::Schema);
        }
    }
    for (manifest, identity) in expected {
        let saved = registry
            .modules
            .iter()
            .find(|m| m.manifest == manifest)
            .ok_or(owners::Error::Schema)?;
        if saved.executable != kp::Executable::NativeV1 || saved.target.is_some() {
            return Err(owners::Error::Schema);
        }
        let canonical = serde_json::to_vec(
            &serde_json::to_value((&manifest, &registry.limits, Option::<[u8; 32]>::None))
                .map_err(|_| owners::Error::Schema)?,
        )
        .map_err(|_| owners::Error::Schema)?;
        let mut digest = Sha256::new();
        for part in [b"native-v1".as_slice(), canonical.as_slice(), identity] {
            digest.update((part.len() as u64).to_le_bytes());
            digest.update(part);
        }
        let expected: [u8; 32] = digest.finalize().into();
        if saved.artifact != expected {
            return Err(owners::Error::Schema);
        }
    }
    Ok(())
}

//! V3 gameplay executes the shared algorithm using invocation-only dependency slots.
use super::*;
use crate::actions::wand::{self as algorithm, ports as p};
use nethacked_arena::{ActorId, ActorRecord, ItemId, ItemRecord};
use nethacked_types::{Coord, Direction};
use wand::*;
mod adapters;
pub(crate) mod facts;

pub(super) const PROVIDER: &str = "native-owner-wand-i2-provider-v3";
pub(super) const CALLER: &str = "native-owner-wand-i2-caller-v3";
pub(super) const RANDOM: &str = "native-owner-wand-i2-random-v3";
pub(super) const NATIVE_ID: &[u8] = b"NetHackED M5-I2 gameplay owner facts v3";
pub(super) const CALLER_ID: &[u8] = b"NetHackED M5-I2 actual shared gameplay caller v3";
pub(super) const RANDOM_ID: &[u8] = b"NetHackED M5-I2 semantic owned sampler v3";
pub(super) const ENDPOINT: &str = "zap_v3";
const MAX_CALLS: usize = 16384;
const MAX_EVENTS: usize = 1024;
const REPLY_BYTES: usize = 1024 * 1024;
pub(super) const READS: [(&str, &str); 5] = [
    ("Objects", "wand_objects_gameplay_v3"),
    ("Actors", "wand_actors_gameplay_v3"),
    ("Hero", "wand_genocide_gameplay_v3"),
    ("Clock", "wand_clock_gameplay_v3"),
    ("Actors", "wand_actor_genocide_gameplay_v3"),
];
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SemanticDraw {
    Below(u32),
    SignedDie(i32),
    MonsterBelow(u32),
    MonsterDie(u32),
}
impl SemanticDraw {
    fn draw(&self) -> owners::Result<p::WandDraw> {
        use nethacked_data::monsters::MonsterDraw;
        Ok(match *self {
            Self::Below(n) if n > 0 => p::WandDraw::Below(n),
            Self::SignedDie(n) if n > 0 => p::WandDraw::SignedDie(n),
            Self::MonsterBelow(n) if n > 0 => p::WandDraw::Monster(MonsterDraw::Below(n)),
            Self::MonsterDie(n) if n > 0 => p::WandDraw::Monster(MonsterDraw::Die(n)),
            _ => return Err(owners::Error::Schema),
        })
    }
}
impl From<p::WandDraw> for SemanticDraw {
    fn from(d: p::WandDraw) -> Self {
        use nethacked_data::monsters::MonsterDraw;
        match d {
            p::WandDraw::Below(n) => Self::Below(n),
            p::WandDraw::SignedDie(n) => Self::SignedDie(n),
            p::WandDraw::Monster(MonsterDraw::Below(n)) => Self::MonsterBelow(n),
            p::WandDraw::Monster(MonsterDraw::Die(n)) => Self::MonsterDie(n),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ObjectQuery {
    Carried(ActorId),
    Selected(ItemId),
    Floor {
        coord: Coord,
        kind: nethacked_data::ItemKindId,
    },
    Amulet(ActorId),
    Exists(ObjectRef),
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActorQuery {
    Record(ActorId),
    At(Coord),
    Guardians { name: String, include_sound: bool },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FactQuery {
    Objects(ObjectQuery),
    Actors(ActorQuery),
    Genocide { version: u32, offset: u32 },
    GenocideRegistry { version: u32, offset: u32 },
    Clock,
}
impl FactQuery {
    pub(super) fn route(&self) -> (&'static str, &'static str) {
        READS[match self {
            Self::Objects(_) => 0,
            Self::Actors(_) => 1,
            Self::GenocideRegistry { .. } => 2,
            Self::Genocide { .. } => 4,
            Self::Clock => 3,
        }]
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Selected {
    pub class: nethacked_types::ItemClass,
    pub name: String,
    pub definition: Option<nethacked_arena::ItemDefinition>,
    pub buc: nethacked_types::Buc,
    pub enchantment: i8,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FactValue {
    Items(Vec<ItemId>),
    Selected(Option<Selected>),
    Bool(bool),
    Actor(Option<ActorRecord>),
    At(Option<ActorId>),
    Actors(Vec<ActorId>),
    Genocide {
        species: Vec<String>,
        classes: Vec<char>,
        next: Option<u32>,
    },
    Turn(u64),
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WandGameplayAction {
    Zap {
        index: Option<u32>,
        expected: Option<ItemId>,
        direction: Direction,
        energy: u32,
    },
    Cancel,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GameplayError {
    Dependency { slot: usize, abi_code: i32 },
    Protocol,
    Budget,
    Timer(TimerFailure),
    Lifetime(super::item_lifetime::LifetimeError),
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WandGameplayReply {
    pub events: Vec<crate::GameEvent>,
    pub error: Option<GameplayError>,
    pub dependency_calls: usize,
}
pub(super) fn endpoint() -> Endpoint {
    Endpoint {
        max_request: MAX_REQUEST_BYTES as u32,
        max_reply: REPLY_BYTES as u32,
        ..super::endpoint(Access::Write)
    }
}
struct Invocation<'a, 'v> {
    view: &'a CallerView<'v>,
    config: &'a OwnerBridgeConfig,
    calls: std::cell::Cell<usize>,
}
impl Invocation<'_, '_> {
    fn call<T: Serialize, R: DeserializeOwned + Serialize>(
        &self,
        slot: usize,
        q: &T,
    ) -> Result<R, GameplayError> {
        let calls = self.calls.get();
        if calls == MAX_CALLS {
            return Err(GameplayError::Budget);
        }
        self.calls.set(calls + 1);
        let bytes = encode(q, MAX_REQUEST_BYTES).map_err(|_| GameplayError::Protocol)?;
        let reply =
            self.view
                .call_dependency(slot, &bytes)
                .map_err(|e| GameplayError::Dependency {
                    slot,
                    abi_code: e.abi_code(),
                })?;
        decode(&reply, MAX_RESPONSE_BYTES).map_err(|_| GameplayError::Protocol)
    }
    fn read(&self, q: WandQuery) -> Result<WireObservation, GameplayError> {
        q.validate().map_err(|_| GameplayError::Protocol)?;
        let slot = wand_commands::read_routes(self.config)
            .iter()
            .position(|r| *r == q.route())
            .ok_or(GameplayError::Protocol)?
            + 3;
        self.call(
            slot,
            &WandRead {
                query: q,
                expected_owner: None,
            },
        )
    }
    fn facts(&self, q: FactQuery) -> Result<FactValue, GameplayError> {
        match self
            .read(WandQuery::Command(CommandQuery::Gameplay(q)))?
            .facts
        {
            WandFacts::Command(CommandFacts::Gameplay(f)) => Ok(f),
            _ => Err(GameplayError::Protocol),
        }
    }
    fn command(&self, command: WandOwnerCommand) -> Result<OperationResult, GameplayError> {
        let owner_query = match &command {
            WandOwnerCommand::Objects(_) => WandQuery::Command(CommandQuery::Gameplay(
                FactQuery::Objects(ObjectQuery::Carried(self.player()?)),
            )),
            WandOwnerCommand::Actors(_) => WandQuery::Command(CommandQuery::Gameplay(
                FactQuery::Actors(ActorQuery::At(Coord { x: 0, y: 0 })),
            )),
            WandOwnerCommand::Geography { .. } => WandQuery::Geography(GeographyRequest {
                residency: Residency::Active,
                cells: vec![],
                trajectory: None,
            }),
            WandOwnerCommand::Hero(_) => WandQuery::Hero,
            WandOwnerCommand::Clock(_) => {
                WandQuery::Command(CommandQuery::Gameplay(FactQuery::Clock))
            }
            WandOwnerCommand::Random { .. } | WandOwnerCommand::Sample { .. } => {
                WandQuery::Random { draws: 1 }
            }
        };
        let expected_owner = self.read(owner_query)?.witness;
        let mut dependencies = vec![];
        for q in wand_commands::dependencies(&command) {
            let o = self.read(q)?;
            dependencies.extend(o.dependencies);
            dependencies.push(o.witness);
        }
        let slot = 3
            + wand_commands::read_routes(self.config).len()
            + wand_commands::command_index(&command).ok_or(GameplayError::Protocol)?;
        self.call(
            slot,
            &WandOperation {
                expected_owner,
                dependencies,
                command,
            },
        )
    }
    fn player(&self) -> Result<ActorId, GameplayError> {
        match self.read(WandQuery::Hero)?.facts {
            WandFacts::Hero(h) => Ok(h.player),
            _ => Err(GameplayError::Protocol),
        }
    }
}
pub(super) fn caller_ports(
    mut ports: BTreeMap<String, NativePort>,
    c: &OwnerBridgeConfig,
) -> BTreeMap<String, NativePort> {
    if c.wand_gameplay {
        let c = c.clone();
        ports.insert(
            ENDPOINT.into(),
            Rc::new(move |bytes, view| {
                if view.operation().is_none_or(|o| {
                    o.phase() != ENDPOINT
                        && !(c.wand_workflows
                            && o.phase() == super::wand_workflows::EXECUTE
                            && super::wand_workflows::artifact(&c)
                                .ok()
                                .is_some_and(|a| view.caller_artifact() == Some(a)))
                }) {
                    return Err(kernel::Error::Authority);
                }
                let transformed;
                let bytes = if c.wand_extensions {
                    let slot = profile_caller(&c)
                        .dependencies
                        .len()
                        .checked_sub(1)
                        .ok_or(kernel::Error::MissingDependency)?;
                    transformed = view.call_dependency(slot, bytes)?;
                    transformed.as_slice()
                } else {
                    bytes
                };
                let action: WandGameplayAction =
                    decode(bytes, MAX_REQUEST_BYTES).map_err(native_refusal)?;
                let invocation = Invocation {
                    view,
                    config: &c,
                    calls: std::cell::Cell::new(0),
                };
                let result = run(&invocation, action);
                encode(&result, REPLY_BYTES).map_err(native_refusal)
            }),
        );
    }
    ports
}
fn run(i: &Invocation<'_, '_>, action: WandGameplayAction) -> WandGameplayReply {
    let result = (|| {
        let WandGameplayAction::Zap {
            index,
            expected,
            direction,
            energy,
        } = action
        else {
            return Ok(vec![crate::GameEvent::LogMessage {
                text: "Never mind.".into(),
            }]);
        };
        if energy as usize > MAX_CELLS {
            return Err(algorithm::WandFailure {
                error: algorithm::WandError::Authority(GameplayError::Budget),
                events: vec![],
            });
        }
        let setup = (|| -> Result<_, GameplayError> {
            let player = i.player()?;
            let session = match i.read(WandQuery::Session)?.facts {
                WandFacts::Session(s) if s.ruleset == i.config.ruleset_ref => s,
                _ => return Err(GameplayError::Protocol),
            };
            let geo = match i
                .read(WandQuery::Geography(GeographyRequest {
                    residency: Residency::Active,
                    cells: vec![],
                    trajectory: None,
                }))?
                .facts
            {
                WandFacts::Geography(g) => g,
                _ => return Err(GameplayError::Protocol),
            };
            let role = match i.read(WandQuery::Command(CommandQuery::Session))?.facts {
                WandFacts::Command(CommandFacts::Session { role_name, .. }) => role_name,
                _ => return Err(GameplayError::Protocol),
            };
            Ok((player, session, geo, role))
        })()
        .map_err(|e| algorithm::WandFailure {
            error: algorithm::WandError::Authority(e),
            events: vec![],
        })?;
        let (player, s, g, role) = setup;
        let session = p::Session {
            ruleset: &i.config.ruleset,
            locale: s.locale,
            branch: g.branch,
            depth: g.depth,
        };
        let outcome = algorithm::try_zap_with_survivor(
            index.map_or(algorithm::Selection::First, |index| {
                algorithm::Selection::Selected { index, expected }
            }),
            player,
            direction,
            energy,
            &mut adapters::Objects(i),
            &mut adapters::Actors(i),
            &mut adapters::Geography(i),
            &mut adapters::Hero {
                invocation: i,
                role,
            },
            &mut adapters::Clock(i),
            &mut adapters::Random(i),
            &session,
        )?;
        let mut events = outcome.events;
        if i.config.item_lifetime && outcome.survivor.is_some() {
            let action = super::item_lifetime::LifetimeAction {
                survivor: outcome.survivor.map(|id| ObjectRef {
                    id,
                    residency: Residency::Active,
                }),
            };
            let result: Result<
                super::item_lifetime::LifetimeReply,
                super::item_lifetime::LifetimeError,
            > = i
                .call(super::item_lifetime::dependency_slot(i.config), &action)
                .map_err(|error| algorithm::WandFailure {
                    error: algorithm::WandError::Authority(error),
                    events: events.clone(),
                })?;
            match result {
                Ok(super::item_lifetime::LifetimeReply::Unchanged) => {}
                Ok(super::item_lifetime::LifetimeReply::Retired { .. }) => {
                    events.push(crate::GameEvent::LogMessage {
                        text: "Your wand turns to dust.".into(),
                    })
                }
                Err(error) => {
                    return Err(algorithm::WandFailure {
                        error: algorithm::WandError::Authority(GameplayError::Lifetime(error)),
                        events,
                    })
                }
            }
        }
        Ok(events)
    })();
    let (events, error) = match result {
        Ok(e) => (e, None),
        Err(e) => (
            e.events,
            Some(match e.error {
                algorithm::WandError::Authority(e) => e,
                algorithm::WandError::Timer(e) => GameplayError::Timer(e.into()),
            }),
        ),
    };
    // Algorithm effects have finite ray/actor budgets; refusing oversized output
    // is explicit and does not replay the applied prefix.
    let error = if events.len() > MAX_EVENTS {
        Some(GameplayError::Budget)
    } else {
        error
    };
    WandGameplayReply {
        events,
        error,
        dependency_calls: i.calls.get(),
    }
}
impl OwnerBridge {
    /// Dispatch the implemented wand actions; other commands remain M6 work.
    pub fn dispatch_wand_action(
        &self,
        action: nethacked_core::ActionAst,
    ) -> Result<WandGameplayReply, OwnerBridgeError> {
        use nethacked_core::ActionAst;
        let action = match action {
            ActionAst::ZapWand { dir, energy } => WandGameplayAction::Zap {
                index: None,
                expected: None,
                direction: dir,
                energy,
            },
            ActionAst::ZapSelectedWand {
                item_index,
                expected_item,
                dir,
                energy,
            } => WandGameplayAction::Zap {
                index: Some(item_index),
                expected: expected_item,
                direction: dir,
                energy,
            },
            ActionAst::CancelZapSelection => WandGameplayAction::Cancel,
            _ => return Err(owners::Error::Unsupported.into()),
        };
        self.dispatch_wand(action)
    }
    pub fn dispatch_wand(
        &self,
        action: WandGameplayAction,
    ) -> Result<WandGameplayReply, OwnerBridgeError> {
        self.dispatch_wand_scoped(action, Scope::All)
    }
    pub fn dispatch_wand_scoped(
        &self,
        action: WandGameplayAction,
        scope: Scope,
    ) -> Result<WandGameplayReply, OwnerBridgeError> {
        if !self.config.wand_gameplay {
            return Err(owners::Error::Unsupported.into());
        }
        let bytes = encode(&action, MAX_REQUEST_BYTES)?;
        let response = self.session.with_runtime(&self.credential.borrow(), |b| {
            let caller = bridge_caller(b, &self.config)?;
            let manifest = b.kernel()?.manifest(caller)?.clone();
            if !approved_profiles(&self.config)
                .iter()
                .any(|profile| profile.wand_gameplay && manifest == profile_caller(profile))
            {
                return Err(owners::Error::Unsupported.into());
            }
            let cleanup = b.host().cleanup_identity()?;
            let (lease, operation) = b.update_kernel(|k| {
                let lease = k.capture_pipeline(
                    caller,
                    ENDPOINT,
                    Authority {
                        access: Access::Write,
                        scope: scope.clone(),
                    },
                )?;
                let id = k.issue_operation()?;
                let operation =
                    k.operation_binding_with_cleanup(id, lease, 1, ENDPOINT, &[cleanup])?;
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
            Ok::<_, OwnerBridgeError>(decode(&result?, REPLY_BYTES)?)
        })?;
        self.session
            .replace_configuration(&self.key, restore_configuration(&self.config)?)?;
        self.credential
            .replace(self.session.issue_credential(&self.key)?);
        response
    }
}

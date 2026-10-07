use rand::Rng;
use std::sync::Arc;

use nethacked_arena::{ActorId, EntityArena, ItemId, ItemLocation};
use nethacked_core::{
    armor_base_ac, energy::SchedulerState, find_ac, nutrition::hunger_of_nutrition, HungerState,
    SpellKind,
};
use nethacked_data::ruleset::{Ruleset, RulesetRef};
use nethacked_data::{CharacterConfig, ItemKindId, MonsterSpeciesId, RoleId};
use nethacked_dungeon::{generate_dungeon_level, DungeonLevel, RoomType};
use nethacked_types::{Buc, Coord, ItemClass};
/// Constitution used for starvation thresholds (`eat.c:3437`); the sim has no
/// Con attribute yet, so every hero uses this documented default.
pub const DEFAULT_PLAYER_CON: i32 = 10;

use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};

use crate::events::GameEvent;

/// Stored state of a dungeon level when player travels to other floors.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredLevel {
    pub level: DungeonLevel,
    pub monsters: Vec<(nethacked_arena::ActorId, nethacked_arena::ActorRecord)>,
    pub items: Vec<(ItemId, nethacked_arena::ItemRecord)>,
    pub unpaid_items: Vec<(ItemId, u32)>,
}

/// Historical source receipt identity; never an active shopkeeper lookup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BillCreditor {
    pub branch: nethacked_types::BranchId,
    pub depth: usize,
    pub shopkeeper: ActorId,
}

/// Source-provided historical facts. None means unavailable, never paid or zero.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BillTerms {
    pub billed_quantity: Option<u32>,
    pub unit_price: Option<u32>,
    pub creditor: Option<BillCreditor>,
    pub legacy_base_cost: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsumedBillRecord {
    pub origin: ItemId,
    pub object: nethacked_arena::ItemRecord,
    pub terms: BillTerms,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BillingState {
    pub live_terms: Vec<(ItemId, BillTerms)>,
    pub consumed: Vec<ConsumedBillRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BillError {
    AlreadyConsumed,
    StaleItem,
    NotCarried,
    InvalidDefinition,
    ZeroQuantity,
    DuplicateLedger,
    InvalidReceipt,
    UnsupportedBillingScope,
    LifecycleReferencesPresent,
    ReceiptAlreadyRecorded,
    SettlementDeferred,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WholeDisposal {
    Unbilled { origin: ItemId },
    RetainedBilled { origin: ItemId },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BillValuation {
    Known {
        total: u64,
    },
    UnresolvedTerms {
        base: u32,
        current_consumed_quantity: Option<u32>,
    },
}

/// Live ledger order followed by detached consumption order. Missing live
/// objects remain visible as unresolved obligations rather than disappearing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BillView {
    pub origin: ItemId,
    pub object: Option<nethacked_arena::ItemRecord>,
    pub terms: BillTerms,
    pub outstanding: bool,
    pub useup: bool,
    pub valuation: BillValuation,
}

// Immutable preflight ticket; consumed immediately by the same exclusive call.
struct PreparedWholeDisposal {
    origin: ItemId,
    object: nethacked_arena::ItemRecord,
    terms: Option<BillTerms>,
}

fn default_ruleset() -> Arc<Ruleset> {
    Ruleset::vanilla()
}

fn default_ruleset_ref() -> RulesetRef {
    RulesetRef::vanilla()
}

/// Error returned when loading a simulation save fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadError {
    Json(String),
    RulesetMismatch {
        expected: Box<RulesetRef>,
        found: Box<RulesetRef>,
    },
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Json(msg) => write!(f, "JSON load error: {msg}"),
            LoadError::RulesetMismatch { expected, found } => {
                write!(
                    f,
                    "Ruleset mismatch: expected {expected:?}, found {found:?}"
                )
            }
        }
    }
}

impl std::error::Error for LoadError {}

/// The complete, deterministic game simulation world.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationWorld {
    #[serde(skip, default = "default_ruleset")]
    pub ruleset: Arc<Ruleset>,
    #[serde(default = "default_ruleset_ref")]
    pub ruleset_ref: RulesetRef,
    pub level: DungeonLevel,
    pub levels: Vec<DungeonLevel>,
    pub current_branch: nethacked_types::BranchId,
    pub stored_levels: Vec<((nethacked_types::BranchId, usize), StoredLevel)>,
    pub depth: usize,
    pub arena: EntityArena,
    pub player_id: ActorId,
    pub wielded_item: Option<ItemId>,
    /// Actual worn armor and single-item negative-multi dressing owner.
    #[serde(default)]
    pub equipment: crate::actions::equipment::EquipmentState,
    pub scheduler: SchedulerState,
    /// Modern saves always emit the owner, including explicit empty/cancelled
    /// queues. Absent legacy scheduling migration remains a separate step.
    #[serde(default)]
    pub object_timers: crate::object_timers::ObjectTimerQueue,
    /// Unpaid shop merchandise as `(item, base cost)`; see [`Self::get_unpaid_cost`].
    pub unpaid_items: Vec<(ItemId, u32)>,
    #[serde(default)]
    pub billing: BillingState,
    pub player_gold: u32,
    pub player_nutrition: i32,
    pub player_pw: u32,
    pub player_max_pw: u32,
    pub known_spells: Vec<(SpellKind, u32)>,
    pub divine_state: nethacked_types::DivineState,
    pub divine_protection: u32,
    /// Temple priest `cheapskate_count` (C `priest.c:562`); one counter for all priests.
    #[serde(default)]
    pub priest_cheapskate: u32,
    pub player_luck: i32,
    pub hero: nethacked_types::Hero,
    pub locale: nethacked_types::Locale,
    pub bones_storage: Vec<nethacked_types::BonesData>,
    #[serde(default)]
    pub candelabrum_state: nethacked_core::CandelabrumState,
    #[serde(default)]
    pub ritual_progress: nethacked_core::RitualProgress,
    #[serde(default)]
    pub vibrating_square: Option<Coord>,
    #[serde(default)]
    pub quest_state: nethacked_core::QuestState,
    #[serde(default)]
    pub alignment_record: i32,
    /// Hero race (C `gu.urace`), used by `peace_minded`'s race rules
    /// (`race_peaceful`/`race_hostile`, makemon.c:2283-2286).
    #[serde(default)]
    pub hero_race: nethacked_data::RaceId,
    /// C `context.mysteryforce`: decay counter of the Mysterious Force
    /// (`do.c:1543,1563`); grows by `rn2(diff + 2)` each time it triggers.
    #[serde(default)]
    pub mysterious_force_count: u32,
    #[serde(default)]
    pub role_name: String,
    /// Config identity; old saves retain omission and decode their stored identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role_id: Option<RoleId>,
    #[serde(default = "default_rng")]
    pub rng: ChaCha8Rng,
    pub seed: u64,
    pub event_log: Vec<GameEvent>,
    #[serde(default)]
    pub max_event_log_len: Option<usize>,
    #[serde(default)]
    pub genocide_registry: nethacked_types::GenocideRegistry,
    #[serde(default)]
    pub conducts: nethacked_types::ConductTracker,
    /// Hero attributes and experience (C `u.acurr`, `u.uexp`, `u.uhpinc`).
    #[serde(default)]
    pub progress: crate::progress::HeroProgress,
}

/// The sim's fallback starting alignment record when a role definition is missing
/// (C starts at `urole.initrecord`, attrib.c:1094; roles default to 10).
pub const INITIAL_ALIGNMENT_RECORD: i32 = 10;

/// Default maximum length of the sliding window for `event_log` (prevents unbounded memory growth).
pub const DEFAULT_MAX_EVENT_LOG_LEN: usize = 10_000;

pub fn default_rng() -> ChaCha8Rng {
    ChaCha8Rng::seed_from_u64(0)
}

impl SimulationWorld {
    /// Owned geography/vitals context for initialized corpse births. Generation
    /// and gameplay share selection facts; only in_mklev changes rot timing.
    pub(crate) fn item_birth_context(
        &self,
        branch: nethacked_types::BranchId,
        depth: usize,
        in_mklev: bool,
    ) -> (
        nethacked_data::containers::ContainerContext,
        nethacked_data::generation::MonsterGenContext,
    ) {
        use nethacked_data::generation::{DungeonAlign, MonsterGenContext};
        use nethacked_types::BranchId;
        let hero_level = self.arena.actors.get(self.player_id).map_or(1, |p| p.level);
        (
            nethacked_data::containers::ContainerContext {
                hero_level,
                level_difficulty: depth.min(u32::MAX as usize) as u32,
                moves: self.scheduler.turn,
                in_mklev,
            },
            MonsterGenContext {
                hero_level: hero_level.min(i32::MAX as u32) as i32,
                level_difficulty: depth.min(i32::MAX as usize) as i32,
                in_hell: branch == BranchId::Gehennom,
                align: if branch == BranchId::GnomishMines {
                    DungeonAlign::Lawful
                } else {
                    DungeonAlign::None
                },
                temperature: 0,
            },
        )
    }

    /// Current sim geography exposes Astral only; elemental planes can be
    /// supplied directly to the data factory when that geography is modeled.
    pub(crate) fn monster_spawn_context(&self) -> nethacked_data::monsters::MonsterSpawnContext {
        nethacked_data::monsters::MonsterSpawnContext {
            endgame_plane: (self.current_branch == nethacked_types::BranchId::AstralPlane)
                .then_some(nethacked_types::EndgamePlane::Astral),
        }
    }

    /// Default maximum length of the sliding window for `event_log` (prevents unbounded memory growth).
    pub const DEFAULT_MAX_EVENT_LOG_LEN: usize = DEFAULT_MAX_EVENT_LOG_LEN;

    /// Remove an actor, leaving anything it carried on the floor where it stood.
    pub(crate) fn remove_actor_dropping_items(&mut self, id: ActorId) {
        let Some(coord) = self.arena.actors.get(id).map(|a| a.coord) else {
            return;
        };
        for item_id in self.arena.items_carried_by(id) {
            if let Some(item) = self.arena.items.get_mut(item_id) {
                item.location = ItemLocation::Floor(coord);
            }
        }
        self.arena.actors.remove(id);
    }

    pub(crate) fn actor_is_genocided(&self, name: &str) -> bool {
        let lower = name.to_lowercase();
        let class = self.ruleset.monster_class_of(name);
        self.genocide_registry.genocided_species.contains(&lower)
            || class
                .map(|c| self.genocide_registry.genocided_classes.contains(&c))
                .unwrap_or(false)
    }

    /// Initialize a new deterministic simulation world with a custom character configuration and ruleset.
    pub fn new_with_character_and_ruleset(
        seed: u64,
        config: CharacterConfig,
        ruleset: Arc<Ruleset>,
        ruleset_ref: RulesetRef,
    ) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let level = generate_dungeon_level(&mut rng);
        let mut arena = EntityArena::new();

        // Spawn player with character configuration (role, race, starting items)
        let (player_id, starting_items) =
            ruleset.spawn_player_character(&config, level.stairs_up, &mut arena);

        let mut unpaid_items = Vec::new();
        let player_gold = if config.role == RoleId::Tourist {
            200
        } else {
            50
        };

        // Nutrition & Mana by role
        let player_nutrition = 900i32;
        let (player_pw, player_max_pw, known_spells) = match config.role {
            RoleId::Wizard => (25, 25, vec![(SpellKind::ForceBolt, 20000)]),
            RoleId::Healer => (20, 20, vec![(SpellKind::CureLightWounds, 20000)]),
            RoleId::Knight | RoleId::Monk => (10, 10, Vec::new()),
            _ => (5, 5, Vec::new()),
        };

        // Spawn starter food in player's pack
        if let Some(item_rec) = ruleset.create_item_record_by_id(
            ItemKindId::FOOD_RATION,
            ItemLocation::CarriedBy(player_id),
            Buc::Uncursed,
        ) {
            arena.spawn_item(item_rec);
        }

        let init_align = ruleset
            .role(config.role)
            .map(|r| r.initial_alignment_record)
            .unwrap_or(INITIAL_ALIGNMENT_RECORD);

        // Process rooms: spawn Shopkeeper and merchandise in Shop, monsters in Normal rooms
        for (i, room) in level.rooms.iter().enumerate() {
            match room.room_type {
                RoomType::Shop => {
                    // Spawn peaceful Shopkeeper
                    if let Some(mut sk) = ruleset.create_monster_record_by_id(
                        MonsterSpeciesId::SHOPKEEPER,
                        room.center(),
                        &mut rng,
                    ) {
                        if let Some(sk_def) = ruleset.monster_by_id(MonsterSpeciesId::SHOPKEEPER) {
                            sk.malign = nethacked_core::calculate_malign(
                                sk_def.maligntyp,
                                config.alignment,
                                sk.is_peaceful,
                                false,
                                sk_def.peaceful_by_default,
                                sk_def.always_hostile,
                            );
                        }
                        arena.spawn_actor(sk);
                    }

                    // Spawn shop merchandise on floor
                    let items_to_sell = [
                        ItemKindId::POT_EXTRA_HEALING,
                        ItemKindId::SCR_IDENTIFY,
                        ItemKindId::LONG_SWORD,
                        ItemKindId::WAN_STRIKING,
                        ItemKindId::LEATHER_ARMOR,
                    ];
                    let mut off = 1;
                    for &kind in &items_to_sell {
                        let ix = (room.x1 + off).min(room.x2.saturating_sub(1));
                        let iy = (room.y1 + 1).min(room.y2.saturating_sub(1));
                        let ic = Coord::new_unchecked(ix, iy);
                        if ic != room.center() {
                            if let Some(item_rec) = ruleset.create_item_record_by_id(
                                kind,
                                ItemLocation::Floor(ic),
                                Buc::Uncursed,
                            ) {
                                let cost = ruleset.item_by_id(kind).map(|it| it.cost).unwrap_or(30);
                                let item_id = arena.spawn_item(item_rec);
                                unpaid_items.push((item_id, cost));
                            }
                        }
                        off += 1;
                    }
                }
                RoomType::Normal if i > 0 && i != level.rooms.len() - 1 => {
                    if let Some(mut goblin) = ruleset.create_monster_record_by_id(
                        MonsterSpeciesId::GOBLIN,
                        room.center(),
                        &mut rng,
                    ) {
                        if let Some(gob_def) = ruleset.monster_by_id(MonsterSpeciesId::GOBLIN) {
                            let input = crate::peace::peace_input(
                                gob_def,
                                config.alignment,
                                config.race,
                                init_align,
                                false,
                            );
                            goblin.is_peaceful = crate::peace::roll_peace_minded(&input, &mut rng);
                            goblin.malign = nethacked_core::calculate_malign(
                                gob_def.maligntyp,
                                config.alignment,
                                goblin.is_peaceful,
                                false,
                                gob_def.peaceful_by_default,
                                gob_def.always_hostile,
                            );
                        }
                        arena.spawn_actor(goblin);
                    }
                }
                _ => {}
            }
        }

        // Spawn starting floor items near stairs from declarative item catalog
        let item_coord1 =
            Coord::new(level.stairs_up.x + 1, level.stairs_up.y).unwrap_or(level.stairs_up);
        if let Some(it) = ruleset.create_item_record_by_id(
            ItemKindId::SILVER_SABER,
            ItemLocation::Floor(item_coord1),
            Buc::Uncursed,
        ) {
            arena.spawn_item(it);
        }

        let item_coord2 =
            Coord::new(level.stairs_up.x, level.stairs_up.y + 1).unwrap_or(level.stairs_up);
        if let Some(it) = ruleset.create_item_record_by_id(
            ItemKindId::POT_HEALING,
            ItemLocation::Floor(item_coord2),
            Buc::Blessed,
        ) {
            arena.spawn_item(it);
        }

        let item_coord3 =
            Coord::new(level.stairs_up.x + 1, level.stairs_up.y + 1).unwrap_or(level.stairs_up);
        if let Some(it) = ruleset.create_item_record_by_id(
            ItemKindId::BAG_OF_HOLDING,
            ItemLocation::Floor(item_coord3),
            Buc::Uncursed,
        ) {
            arena.spawn_item(it);
        }

        // Auto-wield first starting weapon if any
        let wielded_item = starting_items.iter().copied().find(|&id| {
            arena
                .items
                .get(id)
                .map(|i| i.class == ItemClass::Weapon)
                .unwrap_or(false)
        });

        let starting_skills = ruleset
            .role(config.role)
            .map(|r| r.skills.iter().copied().collect())
            .unwrap_or_else(|| {
                nethacked_data::starting_skills(config.role)
                    .into_iter()
                    .collect()
            });

        let mut sim = Self {
            ruleset,
            ruleset_ref,
            levels: vec![level.clone()],
            current_branch: nethacked_types::BranchId::DungeonsOfDoom,
            stored_levels: Vec::new(),
            depth: 1,
            level,
            player_id,
            wielded_item,
            equipment: crate::actions::equipment::EquipmentState::default(),
            scheduler: SchedulerState::new(12, 0),
            object_timers: crate::object_timers::ObjectTimerQueue::default(),
            unpaid_items,
            billing: BillingState::default(),
            player_gold,
            player_nutrition,
            player_pw,
            player_max_pw,
            known_spells,
            divine_state: nethacked_types::DivineState::default(),
            divine_protection: 0,
            priest_cheapskate: 0,
            player_luck: 0,
            hero: nethacked_types::Hero {
                base_hp: arena.actors[player_id].hp as i32,
                base_max_hp: arena.actors[player_id].max_hp as i32,
                polymorph: None,
                lycanthropy: None,
                afflictions: nethacked_types::AfflictionState::default(),
                skills: nethacked_types::SkillTree {
                    skills: starting_skills,
                    ..nethacked_types::SkillTree::default()
                },
                mount: None,
                quivered_item: None,
            },
            locale: nethacked_types::Locale::En,
            arena,
            bones_storage: Vec::new(),
            candelabrum_state: nethacked_core::CandelabrumState::empty(),
            ritual_progress: nethacked_core::RitualProgress::Uninitiated,
            vibrating_square: None,
            quest_state: nethacked_core::QuestState::default(),
            alignment_record: init_align,
            hero_race: config.race,
            mysterious_force_count: 0,
            role_name: format!("{:?}", config.role),
            role_id: Some(config.role),
            rng,
            seed,
            event_log: Vec::new(),
            max_event_log_len: Some(DEFAULT_MAX_EVENT_LOG_LEN),
            genocide_registry: nethacked_types::GenocideRegistry::default(),
            conducts: nethacked_types::ConductTracker::default(),
            progress: crate::progress::HeroProgress::default(),
        };
        for id in starting_items {
            let Some(item) = sim.arena.items.get(id) else {
                continue;
            };
            if let Some(armor) = sim
                .ruleset
                .item(&item.name)
                .and_then(|def| def.armor.as_ref())
            {
                if sim.equipment.worn(armor.slot).is_none() {
                    sim.equipment.armor.push((armor.slot, id));
                }
            }
        }
        sim.init_hero_attributes();
        sim.recompute_hero_ac();
        sim
    }

    /// Initialize a new deterministic simulation world with a custom character configuration.
    pub fn new_with_character(seed: u64, config: CharacterConfig) -> Self {
        Self::new_with_character_and_ruleset(
            seed,
            config,
            Ruleset::vanilla(),
            RulesetRef::vanilla(),
        )
    }

    /// Serialize the simulation state to JSON string.
    pub fn to_save_json(&self) -> Result<String, serde_json::Error> {
        let mut snapshot = self.clone();
        snapshot.register_existing_artifacts();
        snapshot.reconcile_object_timers();
        serde_json::to_string(&snapshot)
    }

    /// Deserialize a saved simulation world, validating ruleset reference compatibility.
    pub fn from_save_json(
        json: &str,
        rs: Arc<Ruleset>,
        rref: &RulesetRef,
    ) -> Result<Self, LoadError> {
        let value: serde_json::Value =
            serde_json::from_str(json).map_err(|e| LoadError::Json(e.to_string()))?;
        let legacy_equipment = value.get("equipment").is_none();
        let legacy_object_timers = value.get("object_timers").is_none();
        let legacy_billing = value.get("billing").is_none();
        let mut world: SimulationWorld =
            serde_json::from_str(json).map_err(|e| LoadError::Json(e.to_string()))?;
        if &world.ruleset_ref != rref {
            return Err(LoadError::RulesetMismatch {
                expected: Box::new(rref.clone()),
                found: Box::new(world.ruleset_ref),
            });
        }
        world.ruleset = rs;
        // Absence supplies only the empty owner, never historical receipt facts.
        // Explicit owners (including empty ones) are retained and validated.
        if legacy_billing {
            world.billing = BillingState::default();
        }
        world
            .validate_billing_snapshot()
            .map_err(|error| LoadError::Json(format!("billing: {error:?}")))?;
        // Old saves lack these optional tags. Resolve through the owned
        // definition's canonical identity/profile, without guessing by name.
        let restore_tags = |item: &mut nethacked_arena::ItemRecord| {
            if let Some(def) = world.ruleset.item(&item.name) {
                if def.id == Some(ItemKindId::CORPSE) && item.corpse_species.is_none() {
                    item.corpse_species = Some(nethacked_arena::CorpseSpecies::Unspecified);
                }
                if matches!(
                    def.initialization,
                    nethacked_data::item_init::ItemInitProfile::Container {
                        container: nethacked_data::item_init::ContainerInit::IceBox
                    }
                ) {
                    item.is_ice_box = true;
                }
            }
        };
        for (_, item) in world.arena.items.iter_mut() {
            restore_tags(item);
        }
        for (_, stored) in &mut world.stored_levels {
            for (_, item) in &mut stored.items {
                restore_tags(item);
            }
        }
        world.register_existing_artifacts();
        // Before actual worn state existed, old saves represented the first
        // directly carried armor per slot as worn. Migrate only absent owners;
        // an explicit empty owner means the hero really is not wearing armor.
        if legacy_equipment {
            for id in world.arena.items_carried_by(world.player_id) {
                let Some(item) = world.arena.items.get(id) else {
                    continue;
                };
                if item.class != ItemClass::Armor {
                    continue;
                }
                let slot = world
                    .ruleset
                    .item(&item.name)
                    .and_then(|def| def.armor.as_ref().map(|armor| armor.slot))
                    .or_else(|| nethacked_core::armor_slot(&item.name));
                if let Some(slot) = slot {
                    if world.equipment.worn(slot).is_none() {
                        world.equipment.armor.push((slot, id));
                    }
                }
            }
        }
        world.reconcile_equipment();
        world.recompute_hero_ac();
        if !legacy_object_timers {
            world.validate_object_timers().map_err(LoadError::Json)?;
            world.restore_saved_object_timers();
        } else {
            world
                .migrate_absent_corpse_timers()
                .map_err(|error| LoadError::Json(format!("legacy corpse timer: {error:?}")))?;
        }
        // New absent-owner samples are already installed in source insertion
        // order; saved-owner restoration must not reverse their ties again.
        // C restore.c:930 catches up after object references are restored.
        // No clock advance; off-level timers remain deferred.
        let expired = world.run_object_timers();
        if !expired.is_empty() {
            world.record_events(&expired);
        }
        Ok(world)
    }

    /// Register imported, cached and explicitly mutated records without RNG.
    pub fn register_existing_artifacts(&mut self) {
        let rules = &self.ruleset;
        let history = &mut self.arena.artifact_history;
        let mut register = |item: &mut nethacked_arena::ItemRecord| {
            if item.artifact_id.is_none() {
                item.artifact_id = rules.artifact_id_for_item(&item.name).map(str::to_owned);
            }
            if let Some(id) = &item.artifact_id {
                history.insert(id.clone());
            }
        };
        for (_, item) in self.arena.items.iter_mut() {
            register(item);
        }
        for (_, stored) in &mut self.stored_levels {
            for (_, item) in &mut stored.items {
                register(item);
            }
        }
    }

    /// Returns the current coordinate of the player character.
    pub fn hero_coord(&self) -> Coord {
        self.arena
            .actors
            .get(self.player_id)
            .map(|a| a.coord)
            .unwrap_or(Coord::new_unchecked(1, 1))
    }

    /// Compute hero's current AC according to NetHack 5.0 C `find_ac(void)` (`do_wear.c:2473-2507`).
    ///
    /// Human hero has base AC 10 (`mons[u.umonnum].ac`, `do_wear.c:2475`).
    /// Only explicitly worn, directly carried armor contributes. Divine
    /// protection is subtracted as C `u.ublessed`.
    pub fn compute_hero_ac(&self) -> i32 {
        let mut worn_armor = Vec::new();
        for &(slot, id) in &self.equipment.armor {
            let Some(item) = self.arena.items.get(id) else {
                continue;
            };
            if item.location != ItemLocation::CarriedBy(self.player_id) {
                continue;
            }
            let base_ac = self
                .ruleset
                .item(&item.name)
                .and_then(|def| def.armor.as_ref())
                .filter(|armor| armor.slot == slot)
                .map(|armor| armor.base_ac)
                .or_else(|| {
                    (slot != nethacked_core::ac::ArmorSlot::Suit).then(|| armor_base_ac(&item.name))
                });
            if let Some(base_ac) = base_ac {
                worn_armor.push((base_ac, item.enchantment as i32, item.erosion));
            }
        }
        find_ac(10, &worn_armor, self.divine_protection as i32)
    }

    /// Recomputes the hero's AC and updates `player.ac`.
    pub fn recompute_hero_ac(&mut self) {
        let ac = self.compute_hero_ac();
        if let Some(player) = self.arena.actors.get_mut(self.player_id) {
            player.ac = ac;
        }
    }

    /// Set active locale for game event text and logging.
    pub fn set_locale(&mut self, locale: nethacked_types::Locale) {
        self.locale = locale;
    }

    /// Retrieve active locale.
    pub fn get_locale(&self) -> nethacked_types::Locale {
        self.locale
    }

    /// Initialize a new deterministic simulation world from a seed with default character.
    pub fn new_with_seed(seed: u64) -> Self {
        Self::new_with_character(seed, CharacterConfig::default())
    }

    /// Retrieve the price owed for an unpaid shop item, if any: the ledger holds the base
    /// cost (`oc_cost`), priced through C `get_cost` (`shk.c:2877`) for the current hero.
    pub fn get_unpaid_cost(&self, id: ItemId) -> Option<u32> {
        self.unpaid_items
            .iter()
            .find(|(i, _)| *i == id)
            .map(|(_, base)| self.shop_buy_price(*base))
    }

    /// Check if an item is unpaid store merchandise.
    pub fn is_unpaid(&self, id: ItemId) -> bool {
        self.unpaid_items.iter().any(|(i, _)| *i == id)
    }

    /// Clear unpaid status after purchase.
    pub fn remove_unpaid(&mut self, id: ItemId) {
        let _ = self.try_remove_unpaid(id);
    }

    /// Clearing a frozen or consumed obligation requires the future settlement route.
    pub fn try_remove_unpaid(&mut self, id: ItemId) -> Result<(), BillError> {
        if self.billing.live_terms.iter().any(|(i, _)| *i == id)
            || self.billing.consumed.iter().any(|bill| bill.origin == id)
        {
            return Err(BillError::SettlementDeferred);
        }
        self.unpaid_items.retain(|(i, _)| *i != id);
        Ok(())
    }

    pub(crate) fn billing_settlement_deferred(&self) -> bool {
        !self.billing.live_terms.is_empty() || !self.billing.consumed.is_empty()
    }

    fn unknown_bill_terms(base: u32) -> BillTerms {
        BillTerms {
            billed_quantity: None,
            unit_price: None,
            creditor: None,
            legacy_base_cost: base,
        }
    }

    /// Read-only save preflight. Historical origins/creditors are not live lookups.
    fn validate_billing_snapshot(&self) -> Result<(), BillError> {
        let rs = Arc::clone(&self.ruleset);
        let validate_object = |object: &nethacked_arena::ItemRecord| {
            let def = match &object.definition {
                Some(identity) => rs.item_for_definition(identity),
                None => rs.item(&object.name),
            }
            .ok_or(BillError::InvalidDefinition)?;
            if def.class != object.class {
                return Err(BillError::InvalidDefinition);
            }
            if object.class != ItemClass::Wand || object.is_container {
                return Err(BillError::UnsupportedBillingScope);
            }
            if object.quantity == 0 {
                return Err(BillError::ZeroQuantity);
            }
            Ok(())
        };
        let mut seen = std::collections::BTreeSet::new();
        for (id, terms) in &self.billing.live_terms {
            if !seen.insert(*id) {
                return Err(BillError::DuplicateLedger);
            }
            let mut owners = self.arena.items.get(*id).into_iter().chain(
                self.stored_levels
                    .iter()
                    .flat_map(|(_, level)| &level.items)
                    .filter(|(i, _)| i == id)
                    .map(|(_, object)| object),
            );
            let object = owners.next().ok_or(BillError::StaleItem)?;
            if owners.next().is_some() {
                return Err(BillError::DuplicateLedger);
            }
            let mut tuples = self
                .unpaid_items
                .iter()
                .chain(
                    self.stored_levels
                        .iter()
                        .flat_map(|(_, level)| &level.unpaid_items),
                )
                .filter(|(i, _)| i == id);
            let (_, base) = tuples.next().ok_or(BillError::InvalidReceipt)?;
            if tuples.next().is_some() {
                return Err(BillError::DuplicateLedger);
            }
            // The tuple and its object must belong to the same level owner.
            if self.arena.items.contains_key(*id) {
                if !self.unpaid_items.iter().any(|(i, _)| i == id) {
                    return Err(BillError::InvalidReceipt);
                }
            } else if !self.stored_levels.iter().any(|(_, level)| {
                level.items.iter().any(|(i, _)| i == id)
                    && level.unpaid_items.iter().any(|(i, _)| i == id)
            }) {
                return Err(BillError::InvalidReceipt);
            }
            validate_object(object)?;
            if self
                .arena
                .items
                .iter()
                .map(|(_, object)| object)
                .chain(
                    self.stored_levels
                        .iter()
                        .flat_map(|(_, level)| level.items.iter().map(|(_, object)| object)),
                )
                .any(|object| object.location == ItemLocation::InContainer(*id))
            {
                return Err(BillError::UnsupportedBillingScope);
            }
            Self::validate_bill_terms(terms, object.quantity, *base)?;
        }
        for bill in &self.billing.consumed {
            if !seen.insert(bill.origin)
                || self.arena.items.contains_key(bill.origin)
                || self.stored_levels.iter().any(|(_, level)| {
                    level.items.iter().any(|(id, _)| *id == bill.origin)
                        || level.unpaid_items.iter().any(|(id, _)| *id == bill.origin)
                })
                || self.unpaid_items.iter().any(|(id, _)| *id == bill.origin)
            {
                return Err(BillError::DuplicateLedger);
            }
            validate_object(&bill.object)?;
            if bill.object.location != ItemLocation::Limbo {
                return Err(BillError::InvalidReceipt);
            }
            if bill.terms.billed_quantity.is_none() && bill.terms.unit_price.is_none() {
                if bill.terms.creditor.is_some() {
                    return Err(BillError::InvalidReceipt);
                }
            } else {
                Self::validate_bill_terms(
                    &bill.terms,
                    bill.object.quantity,
                    bill.terms.legacy_base_cost,
                )?;
            }
        }
        Ok(())
    }

    fn validate_bill_object(&self, id: ItemId) -> Result<&nethacked_arena::ItemRecord, BillError> {
        if self.billing.consumed.iter().any(|bill| bill.origin == id) {
            return Err(BillError::AlreadyConsumed);
        }
        let object = self.arena.items.get(id).ok_or(BillError::StaleItem)?;
        if object.location != ItemLocation::CarriedBy(self.player_id) {
            return Err(BillError::NotCarried);
        }
        let rs = Arc::clone(&self.ruleset);
        let def = match &object.definition {
            Some(identity) => rs.item_for_definition(identity),
            None => rs.item(&object.name),
        }
        .ok_or(BillError::InvalidDefinition)?;
        if def.class != object.class {
            return Err(BillError::InvalidDefinition);
        }
        if object.class != ItemClass::Wand
            || object.is_container
            || !self.arena.items_in_container(id).is_empty()
        {
            return Err(BillError::UnsupportedBillingScope);
        }
        if object.quantity == 0 {
            return Err(BillError::ZeroQuantity);
        }
        Ok(object)
    }

    fn bill_ledger(&self, id: ItemId) -> Result<(Option<u32>, Option<&BillTerms>), BillError> {
        let tuples: Vec<_> = self
            .unpaid_items
            .iter()
            .chain(
                self.stored_levels
                    .iter()
                    .flat_map(|(_, level)| level.unpaid_items.iter()),
            )
            .filter(|(i, _)| *i == id)
            .collect();
        let receipts: Vec<_> = self
            .billing
            .live_terms
            .iter()
            .filter(|(i, _)| *i == id)
            .collect();
        if tuples.len() > 1
            || receipts.len() > 1
            || self
                .stored_levels
                .iter()
                .any(|(_, level)| level.items.iter().any(|(i, _)| *i == id))
        {
            return Err(BillError::DuplicateLedger);
        }
        if !receipts.is_empty() && tuples.is_empty() {
            return Err(BillError::InvalidReceipt);
        }
        // A carried item must belong to the active compatibility ledger.
        if !tuples.is_empty() && !self.unpaid_items.iter().any(|(i, _)| *i == id) {
            return Err(BillError::InvalidReceipt);
        }
        Ok((
            tuples.first().map(|(_, base)| *base),
            receipts.first().map(|(_, terms)| terms),
        ))
    }

    fn validate_bill_terms(terms: &BillTerms, quantity: u32, base: u32) -> Result<(), BillError> {
        if terms.legacy_base_cost != base {
            return Err(BillError::InvalidReceipt);
        }
        let (Some(q), Some(price)) = (terms.billed_quantity, terms.unit_price) else {
            return Err(BillError::InvalidReceipt);
        };
        if q == 0 || price == 0 || u64::from(q) * u64::from(price) > i64::MAX as u64 {
            return Err(BillError::InvalidReceipt);
        }
        if q != quantity {
            return Err(BillError::UnsupportedBillingScope);
        }
        Ok(())
    }

    /// Admit prescribed source terms once. This does not calculate historical prices.
    pub fn record_bill_terms(&mut self, id: ItemId, terms: BillTerms) -> Result<(), BillError> {
        let object = self.validate_bill_object(id)?;
        let (base, existing) = self.bill_ledger(id)?;
        let base = base.ok_or(BillError::InvalidReceipt)?;
        if existing.is_some() {
            return Err(BillError::ReceiptAlreadyRecorded);
        }
        Self::validate_bill_terms(&terms, object.quantity, base)?;
        self.billing.live_terms.push((id, terms));
        Ok(())
    }

    fn prepare_whole_disposal(&self, id: ItemId) -> Result<PreparedWholeDisposal, BillError> {
        let object = self.validate_bill_object(id)?;
        let (base, receipt) = self.bill_ledger(id)?;
        if let Some(terms) = receipt {
            Self::validate_bill_terms(
                terms,
                object.quantity,
                base.ok_or(BillError::InvalidReceipt)?,
            )?;
        }
        if self.wielded_item == Some(id)
            || self.hero.quivered_item == Some(id)
            || self.equipment.armor.iter().any(|(_, i)| *i == id)
            || self
                .equipment
                .dressing
                .as_ref()
                .is_some_and(|d| d.item == id)
            || self
                .object_timers
                .entries()
                .iter()
                .any(|timer| match timer.target {
                    crate::object_timers::ObjectTimerTarget::Active(i) => i == id,
                    crate::object_timers::ObjectTimerTarget::Cached { item, .. } => item == id,
                })
        {
            return Err(BillError::LifecycleReferencesPresent);
        }
        Ok(PreparedWholeDisposal {
            origin: id,
            object: object.clone(),
            terms: receipt
                .cloned()
                .or_else(|| base.map(Self::unknown_bill_terms)),
        })
    }

    fn commit_whole_disposal(&mut self, mut prepared: PreparedWholeDisposal) -> WholeDisposal {
        let origin = prepared.origin;
        let result = if let Some(terms) = prepared.terms {
            prepared.object.location = ItemLocation::Limbo;
            self.billing.consumed.push(ConsumedBillRecord {
                origin,
                object: prepared.object,
                terms,
            });
            WholeDisposal::RetainedBilled { origin }
        } else {
            WholeDisposal::Unbilled { origin }
        };
        self.arena.items.remove(origin);
        self.unpaid_items.retain(|(i, _)| *i != origin);
        self.billing.live_terms.retain(|(i, _)| *i != origin);
        result
    }

    /// Native whole-carried wand fixture route; no lifecycle cleanup or activation.
    pub fn dispose_whole_carried_item(&mut self, id: ItemId) -> Result<WholeDisposal, BillError> {
        let prepared = self.prepare_whole_disposal(id)?;
        Ok(self.commit_whole_disposal(prepared))
    }

    pub fn outstanding_bills(&self) -> Vec<BillView> {
        let view = |origin,
                    object: Option<nethacked_arena::ItemRecord>,
                    terms: BillTerms,
                    useup| {
            let valuation = match (terms.billed_quantity, terms.unit_price) {
                (Some(q), Some(price))
                    if q > 0 && price > 0 && u64::from(q) * u64::from(price) <= i64::MAX as u64 =>
                {
                    BillValuation::Known {
                        total: u64::from(q) * u64::from(price),
                    }
                }
                _ => BillValuation::UnresolvedTerms {
                    base: terms.legacy_base_cost,
                    current_consumed_quantity: object.as_ref().map(|o| o.quantity),
                },
            };
            BillView {
                origin,
                object,
                terms,
                outstanding: true,
                useup,
                valuation,
            }
        };
        let mut views = Vec::new();
        for &(id, base) in self.unpaid_items.iter().chain(
            self.stored_levels
                .iter()
                .flat_map(|(_, level)| level.unpaid_items.iter()),
        ) {
            let mut objects = self.arena.items.get(id).into_iter().chain(
                self.stored_levels
                    .iter()
                    .flat_map(|(_, level)| level.items.iter())
                    .filter(|(i, _)| *i == id)
                    .map(|(_, object)| object),
            );
            let first = objects.next();
            let object = if objects.next().is_none() {
                first.cloned()
            } else {
                None
            };
            let terms = self
                .billing
                .live_terms
                .iter()
                .find(|(i, _)| *i == id)
                .map(|(_, terms)| terms.clone())
                .unwrap_or_else(|| Self::unknown_bill_terms(base));
            views.push(view(id, object, terms, false));
        }
        for bill in &self.billing.consumed {
            views.push(view(
                bill.origin,
                Some(bill.object.clone()),
                bill.terms.clone(),
                true,
            ));
        }
        views
    }

    /// Clear represented references after the rot callback removes an item.
    /// Full shop bills/mextra and unowned C object references remain separate.
    pub(crate) fn forget_rot_corpse_references(&mut self, id: ItemId) {
        if self.wielded_item == Some(id) {
            self.wielded_item = None;
        }
        if self.hero.quivered_item == Some(id) {
            self.hero.quivered_item = None;
        }
        self.remove_unpaid(id);
        self.remove_destroyed_item_equipment(id);
    }

    /// Retrieve an EconomyLedger snapshot of current unpaid store items.
    pub fn economy_ledger(&self) -> crate::actions::economy::EconomyLedger {
        crate::actions::economy::EconomyLedger::with_items(self.unpaid_items.clone())
    }

    /// Append game events to the event log, maintaining a sliding window limit if configured.
    pub fn record_events(&mut self, events: &[GameEvent]) {
        self.event_log.extend(events.iter().cloned());
        let cap = self
            .max_event_log_len
            .unwrap_or(Self::DEFAULT_MAX_EVENT_LOG_LEN);
        if self.event_log.len() > cap {
            let excess = self.event_log.len() - cap;
            self.event_log.drain(0..excess);
        }
    }

    /// Set or clear the maximum capacity of the event log circular buffer.
    pub fn set_max_event_log_len(&mut self, cap: Option<usize>) {
        self.max_event_log_len = cap;
        if let Some(cap) = cap {
            if self.event_log.len() > cap {
                let excess = self.event_log.len() - cap;
                self.event_log.drain(0..excess);
            }
        }
    }

    /// Find actor occupying a specific coordinate.
    pub fn actor_at(&self, coord: Coord) -> Option<ActorId> {
        self.arena.actors.iter().find_map(|(id, actor)| {
            if actor.coord == coord && !actor.is_dead {
                Some(id)
            } else {
                None
            }
        })
    }

    /// Match SlotMap's restored freelist before revival inserts one actor.
    /// Like cached-level restoration, this preserves every occupied key and
    /// slot version while making a full-save boundary invisible to allocation.
    /// Arena keys are not C global monster IDs or linked-list ordering.
    pub(crate) fn normalize_revival_actor_allocator(&mut self) {
        self.arena.actors = serde_json::from_value(
            serde_json::to_value(&self.arena.actors).expect("actor arena serializes"),
        )
        .expect("serialized actor arena restores");
    }

    /// Return the current hunger state based on nutrition points.
    pub fn hunger_state(&self) -> HungerState {
        hunger_of_nutrition(self.player_nutrition, DEFAULT_PLAYER_CON)
    }

    /// Luck timeout period in turns for the hero's current state (C `timeout.c:595-620`).
    ///
    /// The Amulet of Yendor must be carried. `divine_state` has no god-anger field yet,
    /// so `god_angry` is always false (documented limitation).
    pub fn luck_timeout_period(&self) -> u64 {
        nethacked_core::luck_decay_period(self.hero_has_amulet(), false)
    }

    /// Progress one tick of luck decay based on carried luckstone (C `timeout.c:595-620`).
    ///
    /// Base luck is 0: moon phase / Friday 13th are not tracked.
    pub fn tick_luck_decay(&mut self) {
        let luckstone = self
            .arena
            .items_carried_by(self.player_id)
            .into_iter()
            .find_map(|iid| {
                self.arena
                    .items
                    .get(iid)
                    .filter(|it| it.name.eq_ignore_ascii_case("luckstone"))
                    .map(|it| it.buc)
            });
        self.player_luck = nethacked_core::mines::step_luck_decay(self.player_luck, 0, luckstone);
    }

    /// Compute tile visibility and monster perception for the hero, taking into account:
    /// - Field of View (FOV)
    /// - Light sources (carried lit oil lamps / magic lamps / candles)
    /// - Room darkness
    /// - Blindness intrinsic
    /// - Telepathy intrinsic (sensing minded monsters when blind or in darkness)
    pub fn compute_perception(
        &self,
    ) -> (
        std::collections::HashSet<Coord>,
        std::collections::HashSet<ActorId>,
    ) {
        use std::collections::HashSet;

        let Some(player) = self.arena.actors.get(self.player_id) else {
            return (HashSet::new(), HashSet::new());
        };
        let p_coord = player.coord;
        let is_blind = player.intrinsics.blind;
        let has_telepathy = player.intrinsics.telepathy;

        // 1. Gather all active light sources in the level
        let mut light_sources: Vec<(Coord, u32)> = Vec::new();

        // Check player carried items for lit lamp
        let carried = self.arena.items_carried_by(self.player_id);
        for iid in carried {
            if let Some(it) = self.arena.items.get(iid) {
                if let Some(arch) = self.ruleset.item(&it.name) {
                    if arch.is_light_source() && it.enchantment > 0 {
                        let radius = arch.light_radius();
                        if radius > 0 {
                            light_sources.push((p_coord, radius));
                        }
                    }
                }
            }
        }

        // Compute standard FOV (radius 8)
        let fov = nethacked_dungeon::compute_fov(&self.level, p_coord, 8);
        // Compute illumination from light sources
        let illuminated = nethacked_dungeon::compute_illumination(&self.level, &light_sources);

        // Determine visible tiles
        let mut visible_tiles = HashSet::new();
        for &c in &fov {
            let dist = p_coord.chebyshev_distance(c) as u32;
            let is_dark = self.level.is_dark_at(c);
            let is_lit = illuminated.contains(&c);
            if nethacked_core::lighting::can_see_tile(is_blind, dist, is_dark, is_lit) {
                visible_tiles.insert(c);
            }
        }

        // Determine detected monsters
        let mut detected_monsters = HashSet::new();
        for (aid, actor) in self.arena.actors.iter() {
            if actor.is_dead {
                continue;
            }
            let tile_vis = visible_tiles.contains(&actor.coord);
            let has_mind = nethacked_core::lighting::monster_has_mind(&actor.name);
            if nethacked_core::lighting::can_detect_monster(
                is_blind,
                has_telepathy,
                has_mind,
                tile_vis,
            ) {
                detected_monsters.insert(aid);
            }
        }

        (visible_tiles, detected_monsters)
    }

    /// Apply damage to the hero without underflow; marks death at 0 HP.
    pub fn damage_player(&mut self, amount: u32, cause: &str) -> Vec<GameEvent> {
        let mut events = Vec::new();
        if let Some(p) = self.arena.actors.get_mut(self.player_id) {
            p.hp = p.hp.saturating_sub(amount);
            if p.hp == 0 && !p.is_dead {
                p.is_dead = true;
                events.push(GameEvent::LogMessage {
                    text: format!("You die... killed by {cause}."),
                });
            }
        }
        events
    }

    /// C `erode_armor()` for the hero's worn armor.
    pub(crate) fn erode_hero_armor(
        &mut self,
        kind: crate::progress::ErosionKind,
        events: &mut Vec<GameEvent>,
    ) {
        // Armor erosion logic (simplified - messages only for now)
        match kind {
            crate::progress::ErosionKind::Rust => {
                say(events, "Your armor rusts!");
            }
            crate::progress::ErosionKind::Corrode => {
                say(events, "Your armor corrodes!");
            }
            crate::progress::ErosionKind::Rot => {
                say(events, "Your armor rots!");
            }
        }
    }

    /// C steal.c:409-448 selection and substitutions. Returns true when steal
    /// asks the attacker to flee (successful transfer or empty inventory).
    pub(crate) fn steal_hero_item(
        &mut self,
        attacker: &nethacked_arena::ActorRecord,
        attacker_id: ActorId,
        events: &mut Vec<GameEvent>,
    ) -> bool {
        use nethacked_core::ac::ArmorSlot;
        let suit = self.equipment.worn(ArmorSlot::Suit);
        let cloak = self.equipment.worn(ArmorSlot::Cloak);
        let carried: Vec<_> = self
            .arena
            .items_carried_by(self.player_id)
            .into_iter()
            .filter(|id| {
                !(suit.is_some() && Some(*id) == cloak)
                    && self
                        .arena
                        .items
                        .get(*id)
                        .is_some_and(|item| item.class != ItemClass::Coin)
            })
            .collect();
        let weight = |id| {
            if self.equipment.armor.iter().any(|(_, worn)| *worn == id) {
                5u32
            } else {
                1
            }
        };
        let total: u32 = carried.iter().map(|id| weight(*id)).sum();
        if total == 0 {
            return true;
        }
        let mut choice = self.rng.random_range(0..total);
        let Some(mut id) = carried.into_iter().find(|id| {
            let w = weight(*id);
            if choice < w {
                true
            } else {
                choice -= w;
                false
            }
        }) else {
            return false;
        };
        // steal.c:433-448: steal the outer blocking worn item instead.
        if Some(id) == self.equipment.worn(ArmorSlot::Gloves) && self.wielded_item.is_some() {
            id = self.wielded_item.unwrap();
        } else if Some(id) == suit && cloak.is_some() {
            id = cloak.unwrap();
        } else if Some(id) == self.equipment.worn(ArmorSlot::Shirt) {
            if let Some(outer) = cloak.or(suit) {
                id = outer;
            }
        }
        self.interrupt_dressing(events);
        if self.equipment.armor.iter().any(|(_, worn)| *worn == id) {
            // Forced delayed removal of selected worn armor remains unsupported;
            // stop_donning is real, but no transfer/flee success is fabricated.
            say(events, "The thief cannot finish removing your worn armor.");
            return false;
        }
        let name = self.arena.items.get(id).unwrap().name.clone();
        self.arena.items.get_mut(id).unwrap().location = ItemLocation::CarriedBy(attacker_id);
        if self.wielded_item == Some(id) {
            self.wielded_item = None;
        }
        if self.hero.quivered_item == Some(id) {
            self.hero.quivered_item = None;
        }
        events.push(GameEvent::ItemStolen {
            actor: attacker_id,
            item: id,
        });
        say(
            events,
            &format!("{} steals your {}!", capitalize(&attacker.name), name),
        );
        true
    }

    /// Ordinary-terrain C rloc (teleport.c:1848-1888): prescribed x/y
    /// draws for 50 attempts, then incremental randomized candidate fallback.
    /// Special monster habitat/teleport-region restrictions are separate gaps.
    pub(crate) fn teleport_theft_attacker(&mut self, id: ActorId, events: &mut Vec<GameEvent>) {
        use nethacked_types::{COLNO, ROWNO};
        if self.level.no_teleport {
            return;
        }
        let Some(actor) = self.arena.actors.get(id).cloned() else {
            return;
        };
        let goodpos = |world: &Self, coord| {
            world.level.is_passable(coord)
                && world.actor_at(coord).is_none_or(|occupant| occupant == id)
        };
        let valid = |world: &Self, coord| {
            goodpos(world, coord)
                && !world.elbereth_scares(&actor, world.level.get_engraving(coord))
        };
        let mut destination = None;
        for _ in 0..50 {
            let x = self.rng.random_range(1..COLNO);
            let y = self.rng.random_range(0..ROWNO);
            let coord = Coord::new_unchecked(x, y);
            if valid(self, coord) {
                destination = Some(coord);
                break;
            }
        }
        if destination.is_none() {
            let mut candidates = Vec::new();
            // collect_coords(CC_INCL_CENTER | CC_UNSHUFFLED | CC_SKIP_MONS):
            // expanding square edges, y before x, centered at <40,10>.
            let cx = COLNO / 2;
            let cy = ROWNO / 2;
            for radius in 0..=cx.max(cy) {
                let lox = cx as isize - radius as isize;
                let hix = cx + radius;
                let loy = cy as isize - radius as isize;
                let hiy = cy + radius;
                for y in loy.max(0) as usize..=hiy.min(ROWNO - 1) {
                    for x in lox.max(1) as usize..=hix.min(COLNO - 1) {
                        if x as isize != lox && x != hix && y as isize != loy && y != hiy {
                            continue;
                        }
                        let coord = Coord::new_unchecked(x, y);
                        // CC_SKIP_MONS uses m_at: the hero remains a candidate;
                        // goodpos rejects that tile only after the shuffle draw.
                        if self.level.is_passable(coord)
                            && self
                                .actor_at(coord)
                                .is_none_or(|occupant| occupant == self.player_id)
                        {
                            candidates.push(coord);
                        }
                    }
                }
            }
            let mut backup = None;
            for i in 0..candidates.len() {
                let j = self.rng.random_range(0..candidates.len() - i);
                candidates.swap(i, i + j);
                let coord = candidates[i];
                if valid(self, coord) {
                    destination = Some(coord);
                    break;
                }
                if goodpos(self, coord) {
                    backup.get_or_insert(coord);
                }
            }
            destination = destination.or(backup);
        }
        if let Some(to) = destination {
            self.arena.actors.get_mut(id).unwrap().coord = to;
            if to != actor.coord {
                events.push(GameEvent::ActorMoved {
                    actor: id,
                    from: actor.coord,
                    to,
                });
            }
        }
    }

    /// Steal gold from the hero (AD_SGLD).
    pub(crate) fn steal_hero_gold(
        &mut self,
        attacker: &nethacked_arena::ActorRecord,
        events: &mut Vec<GameEvent>,
    ) {
        let stolen = (self.player_gold as f32 * 0.1).max(1.0) as u32;
        let stolen = stolen.min(self.player_gold);
        self.player_gold = self.player_gold.saturating_sub(stolen);
        say(
            events,
            &format!("{} steals {} gold!", capitalize(&attacker.name), stolen),
        );
    }

    /// Foocubus seduction (AD_SEDU).
    pub(crate) fn seduce_hero(
        &mut self,
        attacker: &nethacked_arena::ActorRecord,
        attacker_id: ActorId,
        events: &mut Vec<GameEvent>,
    ) {
        if self.rng.random_range(0..2u32) == 0 {
            self.steal_hero_gold(attacker, events);
        } else {
            let _ = self.steal_hero_item(attacker, attacker_id, events);
        }
        // Foocubus teleports away after
        say(events, &format!("{} vanishes!", capitalize(&attacker.name)));
    }

    /// Teleport the hero (AD_TLPT).
    pub(crate) fn teleport_hero(&mut self, events: &mut Vec<GameEvent>) {
        // Simplified - just report teleport
        say(events, "You are teleported!");
    }

    /// Disenchant hero's item (AD_ENCH).
    pub(crate) fn disenchant_hero_item(&mut self, events: &mut Vec<GameEvent>) {
        say(events, "Your equipment feels less effective.");
    }

    /// Polymorph the hero (AD_POLY).
    pub(crate) fn polymorph_hero(&mut self, _dmg: u32, events: &mut Vec<GameEvent>) {
        say(events, "You feel a change coming over you.");
    }

    /// Make hero sick (AD_DISE).
    pub(crate) fn make_sick(&mut self, _dmg: u32, events: &mut Vec<GameEvent>) {
        say(events, "You feel deathly sick.");
    }

    /// Curse hero's items (AD_CURS).
    pub(crate) fn curse_hero_items(&mut self, events: &mut Vec<GameEvent>) {
        say(
            events,
            "You feel a malignant aura surround your possessions.",
        );
    }

    /// Handle clerical attack (AD_CLRC) - angel/demon spells
    pub(crate) fn handle_clerical_attack(
        &mut self,
        _attacker: &nethacked_arena::ActorRecord,
        _dmg: u32,
        events: &mut Vec<GameEvent>,
    ) {
        // Simplified: just report the clerical attack
        say(events, "A spell is cast at you!");
    }

    /// Handle Death's touch (AD_DETH) - instant death chance
    pub(crate) fn handle_death_touch(
        &mut self,
        _attacker: &nethacked_arena::ActorRecord,
        _dmg: u32,
        events: &mut Vec<GameEvent>,
    ) {
        // Death's touch: 3/20 chance of instant death if no magic resistance
        let roll = self.rng.random_range(1..=20u32);
        if roll >= 18 {
            if let Some(hero) = self.arena.actors.get(self.player_id) {
                if !hero.intrinsics.magic_resistance {
                    say(events, "You feel your life force draining away...");
                    self.damage_player(hero.hp, "touch of death");
                    return;
                }
            }
        }
        say(events, "Lucky for you, it didn't work!");
    }

    /// Handle nurse heal (AD_HEAL)
    pub(crate) fn handle_nurse_heal(
        &mut self,
        attacker: &nethacked_arena::ActorRecord,
        events: &mut Vec<GameEvent>,
    ) {
        // First check if hero has armor/weapon
        let carried = self.arena.items_carried_by(self.player_id);
        let has_armor = carried.iter().any(|id| {
            self.arena
                .items
                .get(*id)
                .is_some_and(|it| it.class == nethacked_types::ItemClass::Armor)
        });
        let has_weapon = carried.iter().any(|id| {
            self.arena
                .items
                .get(*id)
                .is_some_and(|it| it.class == nethacked_types::ItemClass::Weapon)
        });

        if !has_armor && !has_weapon {
            if let Some(hero) = self.arena.actors.get_mut(self.player_id) {
                let heal = self.rng.random_range(1..=7u32);
                hero.hp = (hero.hp + heal).min(hero.max_hp);
                say(
                    events,
                    &format!(
                        "{} hits!  (I hope you don't mind.)",
                        capitalize(&attacker.name)
                    ),
                );
                if hero.hp == hero.max_hp && self.rng.random_range(1..=7u32) == 1 {
                    hero.max_hp += 1;
                    say(events, "You feel stronger!");
                }
            }
        } else {
            say(events, "Doc, I can't help you unless you cooperate.");
        }
    }

    /// Steal Amulet/quest artifact (AD_SAMU)
    pub(crate) fn steal_hero_amulet(
        &mut self,
        attacker: &nethacked_arena::ActorRecord,
        events: &mut Vec<GameEvent>,
    ) {
        say(
            events,
            &format!("{} steals the Amulet!", capitalize(&attacker.name)),
        );
    }

    /// Exercise an attribute (for Famine)
    pub(crate) fn exercise_attr(&mut self, attr: usize, gain: bool, events: &mut Vec<GameEvent>) {
        if gain {
            say(
                events,
                &format!(
                    "You feel {}!",
                    match attr {
                        0 => "stronger",
                        3 => "more agile",
                        4 => "healthier",
                        _ => "better",
                    }
                ),
            );
        } else {
            say(
                events,
                &format!(
                    "You feel {}!",
                    match attr {
                        0 => "weaker",
                        3 => "less agile",
                        4 => "sicker",
                        _ => "worse",
                    }
                ),
            );
        }
    }

    /// More hungry (for Famine)
    pub(crate) fn more_hungry(&mut self, amount: i32, events: &mut Vec<GameEvent>) {
        self.player_nutrition = self.player_nutrition.saturating_sub(amount);
        say(events, "You feel very hungry.");
    }
}

fn say(events: &mut Vec<GameEvent>, text: &str) {
    events.push(GameEvent::LogMessage {
        text: text.to_string(),
    });
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}

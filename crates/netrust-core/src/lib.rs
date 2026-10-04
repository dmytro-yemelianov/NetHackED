//! NetRust Core: Safe, modular, deterministic implementation of NetHack mechanics.
//!
//! Formally defined and verified in Lean 4 (`NetMechanics`).

pub mod artifacts_wands;
pub mod ast;
pub mod bones;
pub mod branch;
pub mod buc;
pub mod combat;
pub mod conducts;
pub mod dungeon_stack;
pub mod enchantment;
pub mod endgame;
pub mod energy;
pub mod engraving;
pub mod gehennom;
pub mod grid;
pub mod identification;
pub mod inventory;
pub mod inventory_interaction;
pub mod lighting;
pub mod magic;
pub mod mines;
pub mod monster_abilities;
pub mod nutrition;
pub mod pathfinding;
pub mod pet;
pub mod pet_coop;
pub mod polymorph;
pub mod quest;
pub mod ranged;
pub mod raycast;
pub mod religion;
pub mod sokoban;
pub mod tournament;
pub mod traps;

pub use artifacts_wands::{
    apply_vorpal_strike, parse_wish, recharge_wand, resolve_artifact_damage, zap_wand,
};
pub use ast::{ActionAst, Direction, EffectAst, WorldState};
pub use bones::{corrupt_buc_on_death, create_ghost_hp, is_valid_bones_level};
pub use branch::{branch_entrance_depth, branch_max_depth, enter_branch, exit_branch};
pub use buc::{dip_water, uncurse, Buc, WaterType};
pub use combat::{
    attack_hits, calculate_damage, hero_damage_after_ac, luck_to_hit_bonus, melee_damage,
    monster_to_hit_value, resolve_melee_attack, to_hit_value, AttackResult, Combatant,
};
pub use dungeon_stack::DungeonDepth;
pub use enchantment::{
    apply_erosion, enchant_item, mix_alchemy, EnchantResult, MAX_EROSION, SAFE_ENCHANT_CAP,
};
pub use endgame::{destroy_drawbridge, offer_amulet_on_high_altar, toggle_drawbridge};
pub use energy::{SchedulerState, StepAction, NORMAL_SPEED};
pub use engraving::{is_elbereth_ward_active, Engraving, EngravingMedium};
pub use gehennom::{
    calculate_mysterious_force, is_candelabrum_ready, is_sanctum_accessible, step_ritual,
    CandelabrumState, InvocationStep, RitualProgress, REQUIRED_CANDLES,
};
pub use grid::{Alignment, Coord, DoorState, Tile, COLNO, ROWNO};
pub use identification::{identify_fully, learn_buc, learn_type, KnowledgeLevel};
pub use inventory::{calculate_encumbrance, can_insert_safe, EncumbranceTier, Item, ItemKind};
pub use inventory_interaction::{
    buy_factor, calculate_buy_price, calculate_sell_price, dilute_potion, reverse_price_id,
    rub_lamp, sell_factor, DilutionState, RubResult,
};
pub use lighting::{
    can_detect_monster, can_see_tile, monster_has_mind, tick_light_fuel, LightSource,
    CANDLE_RADIUS, DEFAULT_LAMP_FUEL, LANTERN_RADIUS, OIL_LAMP_RADIUS,
};
pub use magic::{can_cast, cast_spell, decay_retention, mana_cost, SpellKind};
pub use mines::{
    apply_priest_donation, clamp_luck, luck_decay_period, priest_uncurse, protection_donation_cost,
    step_luck_decay, MAX_DIVINE_PROTECTION,
};
pub use monster_abilities::{calculate_summon_count, resolve_breath_damage, resolve_gaze};
pub use netrust_types::{
    ArtifactKind, AscensionOutcome, BonesData, BonesItem, BranchCoord, BranchId, BreathType, Deity,
    DivineState, DrawbridgeState, DrawbridgeTransition, EndgamePlane, GazeEffect, GazeType,
    Intrinsics, MonsterAbility, MonsterSpell, Pantheon, RechargeResult, SacrificeResult,
    WandCharges,
};
pub use nutrition::{hunger_of_nutrition, hunger_tier, metabolic_tick, HungerState};
pub use pathfinding::MetricState;
pub use pet::{feed_pet, interact_with_occupant, swap_displacement, HeroInteraction};
pub use pet_coop::{
    choose_pet_goal, pet_tile_steppable, promote_pet, PetFamily, PetGoal, PetSpeciesTier,
};
pub use polymorph::{FormStats, PolyEntity};
pub use quest::{
    attack_nemesis, consult_leader, get_role_quest_config, is_hero_eligible_for_quest,
    pick_up_quest_artifact, quest_progress_rank, return_to_leader_with_artifact, ArtifactLocation,
    HeroQuestEligibility, QuestProgress, QuestState, RoleQuestConfig, QUEST_MIN_ALIGNMENT,
    QUEST_MIN_LEVEL,
};
pub use raycast::{reflect, step_ray, BeamRay, StepResult, SurfaceOrientation, Velocity};
pub use religion::{clamp_favor, consecrate_water, resolve_sacrifice, tick_prayer_timeout};
pub use sokoban::{push_boulder, PushOutcome};
pub use tournament::{
    calculate_tournament_score, decide_tactical_action, is_hp_critical, TacticalAction,
    TacticalContext,
};

pub mod genocide;
pub mod polypile;
pub use genocide::*;
pub mod afflictions;
pub mod skills;

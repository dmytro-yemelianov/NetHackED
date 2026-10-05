//! Declarative item catalog and archetype registry for NetHackED.

use nethacked_arena::{ItemLocation, ItemRecord};
use nethacked_core::ac::ArmorSlot;
use nethacked_types::ItemClass;
use serde::{Deserialize, Serialize};

/// Wand direction class (C `oc_dir`, `include/objects.h:1445-1500`): `NODIR` wands
/// need no aim, `IMMEDIATE` wands affect the first target hit, `RAY` wands fire a beam.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum WandDir {
    NoDir,
    Immediate,
    Ray,
}

/// Declarative specification of an item archetype (values from C `include/objects.h`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemArchetype {
    pub id: ItemKindId,
    pub name: &'static str,
    pub class: ItemClass,
    pub weight: u32,
    pub cost: u32,
    pub damage_small: (u32, u32),
    pub damage_large: (u32, u32),
    pub ac_bonus: i32,
    pub is_container: bool,
    pub is_bag_of_holding: bool,
    /// C `oc_magic` (`objects.h`), the item is magical.
    #[serde(default)]
    pub oc_magic: bool,
    /// C `oc_dir` for wands; `None` for every other class.
    #[serde(default)]
    pub wand_dir: Option<WandDir>,
    /// C `oc_nutrition` (food; wands carry 30 as in `objects.h:1448`). Corpses use 0 here:
    /// C takes corpse nutrition from the monster.
    #[serde(default)]
    pub nutrition: u32,
    /// Generation weight within the class (C `oc_prob`); 0 for artifacts.
    pub prob: u32,
    /// C material (`objclass.h` `obj_material_types`), lowercase.
    pub material: &'static str,
    /// Armor category (C `oc_armcat`); `None` for non-armor.
    pub armor_slot: Option<ArmorSlot>,
    /// An `artilist[]` artifact (properties of its base object, its own cost).
    pub artifact: bool,
}

include!("generated/items.rs");
/// Look up an item archetype from the catalog table.
pub fn get_item_archetype(id: ItemKindId) -> &'static ItemArchetype {
    &ITEM_CATALOG[id.index()]
}

/// Look up an archetype by its catalog name (`ItemRecord::name` stores no kind id).
pub fn item_archetype_by_name(name: &str) -> Option<&'static ItemArchetype> {
    ITEM_CATALOG
        .iter()
        .find(|item| item.name.eq_ignore_ascii_case(name))
}

use nethacked_types::Buc;

/// Initial charges for a freshly generated wand (stored in `enchantment`).
pub fn initial_wand_charges(id: ItemKindId) -> i8 {
    let arch = get_item_archetype(id);
    if arch.class != nethacked_types::ItemClass::Wand {
        return 0;
    }
    // C mkobj.c:1115-1124: wishing spe=1; NODIR rn1(5,11)=11..15; directional rn1(5,4)=4..8.
    // The catalog has no RNG, so the fixed mid-range values 13 / 6 stand in (spec divergence).
    match (id, arch.wand_dir) {
        (ItemKindId::WAN_WISHING, _) => 1,
        (_, Some(WandDir::NoDir)) => 13,
        _ => 6,
    }
}

/// Factory function to spawn an ItemRecord from declarative archetype data.
pub fn create_item_record(id: ItemKindId, location: ItemLocation, buc: Buc) -> ItemRecord {
    let arch = get_item_archetype(id);
    ItemRecord {
        name: arch.name.to_string(),
        class: arch.class,
        weight: arch.weight,
        buc,
        is_container: arch.is_container,
        is_bag_of_holding: arch.is_bag_of_holding,
        enchantment: initial_wand_charges(id),
        erosion: 0,
        proofed: false,
        location,
        corpse_race: None,
        corpse_age: 0,
        rot_threshold: 50,
        recharged: 0,
    }
}

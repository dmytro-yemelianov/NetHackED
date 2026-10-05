//! Invariants of the tables generated from the NetHack 5.0 C data.

use nethacked_data::items::{ItemKindId, ITEM_CATALOG};
use nethacked_data::monsters::{MonsterSpeciesId, BESTIARY};

#[test]
fn ids_are_table_indices_named_like_c() {
    assert_eq!(BESTIARY.len(), MonsterSpeciesId::COUNT);
    assert_eq!(ITEM_CATALOG.len(), ItemKindId::COUNT);
    for (i, m) in BESTIARY.iter().enumerate() {
        assert_eq!(m.id.index(), i);
    }
    for (i, it) in ITEM_CATALOG.iter().enumerate() {
        assert_eq!(it.id.index(), i);
    }
    assert_eq!(MonsterSpeciesId::ALIGNED_CLERIC.c_name(), "ALIGNED_CLERIC");
    assert_eq!(
        ItemKindId::from_c_name("WAN_WISHING"),
        Some(ItemKindId::WAN_WISHING)
    );
}

#[test]
fn ids_serialize_as_c_names() {
    let json = serde_json::to_string(&ItemKindId::ART_EXCALIBUR).unwrap();
    assert_eq!(json, "\"ART_EXCALIBUR\"");
    let back: ItemKindId = serde_json::from_str(&json).unwrap();
    assert_eq!(back, ItemKindId::ART_EXCALIBUR);
    assert!(serde_json::from_str::<MonsterSpeciesId>("\"NO_SUCH\"").is_err());
}

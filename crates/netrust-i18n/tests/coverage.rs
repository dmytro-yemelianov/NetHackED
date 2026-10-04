use netrust_i18n::{
    t, t_item, t_monster, Messages, ALL_KEYS, UK_IDENTICAL_KEYS, UK_IDENTICAL_NAMES,
};
use netrust_types::Locale;

#[test]
fn every_bestiary_name_is_translated() {
    let missing: Vec<_> = netrust_data::BESTIARY
        .iter()
        .map(|m| m.name)
        .filter(|n| !UK_IDENTICAL_NAMES.contains(&n.to_lowercase().as_str()))
        .filter(|n| t_monster(n, Locale::Uk) == *n)
        .collect();
    assert!(missing.is_empty(), "untranslated monsters: {missing:?}");
}

#[test]
fn every_catalog_item_is_translated() {
    let missing: Vec<_> = netrust_data::ITEM_CATALOG
        .iter()
        .map(|i| i.name)
        .filter(|n| !UK_IDENTICAL_NAMES.contains(&n.to_lowercase().as_str()))
        .filter(|n| t_item(n, Locale::Uk) == *n)
        .collect();
    assert!(missing.is_empty(), "untranslated items: {missing:?}");
}

#[test]
fn every_key_differs_between_locales() {
    let same: Vec<_> = ALL_KEYS
        .iter()
        .filter(|k| !UK_IDENTICAL_KEYS.contains(k))
        .filter(|k| t(k, Locale::En) == t(k, Locale::Uk))
        .collect();
    assert!(same.is_empty(), "keys not translated: {same:?}");
}

#[test]
fn messages_translate_embedded_names() {
    let s = Messages::attack_hit("Hero", "jackal", 3, Locale::Uk);
    assert!(!s.contains("jackal"), "{s}");
    assert!(s.contains(&t_monster("jackal", Locale::Uk)));
    let w = Messages::wish_granted("long sword", Locale::Uk);
    assert!(!w.contains("long sword"), "{w}");
    let c = Messages::divine_crowning("Excalibur", Locale::Uk);
    assert!(!c.contains("Excalibur"), "{c}");
    assert_ne!(
        t_item(
            "cheap plastic imitation of the Amulet of Yendor",
            Locale::Uk
        ),
        "cheap plastic imitation of the Amulet of Yendor"
    );
}

#[test]
fn corpse_suffix_is_case_insensitive() {
    assert_ne!(t_item("Jackal Corpse", Locale::Uk), "Jackal Corpse");
}

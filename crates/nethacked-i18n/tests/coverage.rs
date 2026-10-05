use nethacked_i18n::{
    t, t_item, t_monster, Messages, ALL_KEYS, UK_IDENTICAL_KEYS, UK_IDENTICAL_NAMES,
};
use nethacked_types::Locale;

#[test]
fn every_bestiary_name_is_translated() {
    let missing: Vec<_> = nethacked_data::BESTIARY
        .iter()
        .map(|m| m.name)
        .filter(|n| !UK_IDENTICAL_NAMES.contains(&n.to_lowercase().as_str()))
        .filter(|n| t_monster(n, Locale::Uk) == *n)
        .collect();
    assert!(missing.is_empty(), "untranslated monsters: {missing:?}");
}

#[test]
fn every_catalog_item_is_translated() {
    let missing: Vec<_> = nethacked_data::ITEM_CATALOG
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

#[test]
fn cmd_bar_fits_and_mentions_esc() {
    for loc in [Locale::En, Locale::Uk] {
        let s = t("tui.cmd_bar", loc);
        assert!(
            s.chars().count() <= 80,
            "{loc:?}: {} chars",
            s.chars().count()
        );
        assert!(s.contains("Esc"), "{loc:?}: {s}");
    }
}

#[test]
fn peace_messages_are_translated() {
    assert_eq!(
        Messages::gets_angry("watchman", true, Locale::En),
        "The watchman gets angry!"
    );
    assert_eq!(
        Messages::gets_angry("Medusa", false, Locale::En),
        "Medusa gets angry!"
    );
    let uk = Messages::gets_angry("watchman", true, Locale::Uk);
    assert!(!uk.contains("watchman"), "{uk}");
    for (en, uk) in [
        (
            Messages::sanctum_infidel(Locale::En).to_string(),
            Messages::sanctum_infidel(Locale::Uk).to_string(),
        ),
        (
            Messages::sanctum_be_gone(Locale::En).to_string(),
            Messages::sanctum_be_gone(Locale::Uk).to_string(),
        ),
        (
            Messages::feel_hypocrite(Locale::En).to_string(),
            Messages::feel_hypocrite(Locale::Uk).to_string(),
        ),
        (
            Messages::engraving_fades(Locale::En).to_string(),
            Messages::engraving_fades(Locale::Uk).to_string(),
        ),
        (
            Messages::peaceful_in_the_way("gnome", true, Locale::En),
            Messages::peaceful_in_the_way("gnome", true, Locale::Uk),
        ),
        (
            Messages::peaceful_wont_swap("priest", true, Locale::En),
            Messages::peaceful_wont_swap("priest", true, Locale::Uk),
        ),
        (
            Messages::swap_with_peaceful("gnome", Locale::En),
            Messages::swap_with_peaceful("gnome", Locale::Uk),
        ),
        (
            Messages::guardians_angry_too("warrior", true, Locale::En),
            Messages::guardians_angry_too("warrior", true, Locale::Uk),
        ),
    ] {
        assert_ne!(en, uk);
        assert!(
            !uk.contains("gnome") && !uk.contains("priest") && !uk.contains("warrior"),
            "{uk}"
        );
    }
    assert_eq!(
        Messages::guardians_angry_too("warrior", true, Locale::En),
        "The warriors appear to be angry too..."
    );
}

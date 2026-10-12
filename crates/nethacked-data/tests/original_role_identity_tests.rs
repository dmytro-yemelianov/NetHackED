use nethacked_arena::ActorSpecies;
use nethacked_data::{ruleset::RoleDef, MonsterSpeciesId as M, RoleId, Ruleset};

const ROLES: [(RoleId, M); 9] = [
    (RoleId::Archaeologist, M::ARCHEOLOGIST),
    (RoleId::Barbarian, M::BARBARIAN),
    (RoleId::Healer, M::HEALER),
    (RoleId::Knight, M::KNIGHT),
    (RoleId::Monk, M::MONK),
    (RoleId::Rogue, M::ROGUE),
    (RoleId::Tourist, M::TOURIST),
    (RoleId::Valkyrie, M::VALKYRIE),
    (RoleId::Wizard, M::WIZARD),
];

#[test]
fn nine_source_original_roles_and_legacy_owned_canonical_fallback() {
    let mut rs = (*Ruleset::vanilla()).clone();
    for (role, species) in ROLES {
        assert_eq!(
            rs.role(role).unwrap().original_monster,
            Some(ActorSpecies::Vanilla(species.c_name().into()))
        );
        assert_eq!(
            rs.original_monster_for_role(role).unwrap().id,
            Some(species)
        );
        rs.roles
            .iter_mut()
            .find(|d| d.id == role)
            .unwrap()
            .original_monster = None;
        rs.monsters
            .iter_mut()
            .find(|d| d.id == Some(species))
            .unwrap()
            .name = format!("Renamed {role:?}");
    }
    rs.reindex();
    for (role, species) in ROLES {
        assert_eq!(
            rs.original_monster_for_role(role).unwrap().id,
            Some(species)
        );
        assert_eq!(
            rs.original_monster_for_role(role).unwrap().name,
            format!("Renamed {role:?}")
        );
    }
}

#[test]
fn optional_original_role_keeps_legacy_canonical_hash_input_and_new_metadata_differs() {
    let rs = Ruleset::vanilla();
    let explicit = serde_json::to_value(&*rs).unwrap();
    let mut legacy = explicit.clone();
    for role in legacy["roles"].as_array_mut().unwrap() {
        role.as_object_mut().unwrap().remove("original_monster");
        let decoded: RoleDef = serde_json::from_value(role.clone()).unwrap();
        assert_eq!(decoded.original_monster, None);
        assert_eq!(serde_json::to_value(decoded).unwrap(), *role);
    }
    let loaded: Ruleset = serde_json::from_value(legacy.clone()).unwrap();
    // The canonical byte sequence is the exact SHA-256 input of pack::ruleset_hash.
    assert_eq!(
        serde_json::to_vec(&serde_json::to_value(loaded).unwrap()).unwrap(),
        serde_json::to_vec(&legacy).unwrap()
    );
    assert_ne!(
        serde_json::to_vec(&explicit).unwrap(),
        serde_json::to_vec(&legacy).unwrap()
    );
}

#[test]
fn explicit_custom_override_and_missing_identity_never_infer_names() {
    let mut rs = (*Ruleset::vanilla()).clone();
    let mut custom = rs.monster_by_id(M::GIANT_ANT).unwrap().clone();
    custom.id = None;
    custom.name = "Owned original".into();
    rs.monsters.push(custom);
    let role = rs
        .roles
        .iter_mut()
        .find(|d| d.id == RoleId::Healer)
        .unwrap();
    role.name = "Renamed profession".into();
    role.original_monster = Some(ActorSpecies::Custom("owned original".into()));
    rs.reindex();
    assert_eq!(
        rs.original_monster_for_role(RoleId::Healer).unwrap().name,
        "Owned original"
    );
    let role = rs
        .roles
        .iter_mut()
        .find(|d| d.id == RoleId::Healer)
        .unwrap();
    role.original_monster = Some(ActorSpecies::Vanilla("MISSING".into()));
    rs.reindex();
    assert!(rs.original_monster_for_role(RoleId::Healer).is_none());
    assert!(rs
        .monster_for_species(&ActorSpecies::Custom("healer".into()))
        .is_none());
    rs.roles.retain(|d| d.id != RoleId::Healer);
    rs.reindex();
    assert!(rs.original_monster_for_role(RoleId::Healer).is_none());
}

#[cfg(feature = "schema")]
#[test]
fn original_role_schema_describes_exact_optional_species_wire_shape() {
    let schema = serde_json::to_value(schemars::schema_for!(RoleDef)).unwrap();
    let variants = schema["definitions"]["OriginalMonsterSchema"]["oneOf"]
        .as_array()
        .unwrap();
    assert_eq!(variants.len(), 2);
    for (index, key) in ["Vanilla", "Custom"].into_iter().enumerate() {
        let actual = if index == 0 {
            ActorSpecies::Vanilla("owned-key".into())
        } else {
            ActorSpecies::Custom("owned-key".into())
        };
        let expected = if index == 0 {
            serde_json::json!({"Vanilla": "owned-key"})
        } else {
            serde_json::json!({"Custom": "owned-key"})
        };
        assert_eq!(serde_json::to_value(actual).unwrap(), expected);
        assert_eq!(variants[index]["properties"][key]["type"], "string");
        assert_eq!(variants[index]["required"], serde_json::json!([key]));
    }
    assert!(!schema["required"]
        .as_array()
        .unwrap()
        .contains(&serde_json::json!("original_monster")));
}

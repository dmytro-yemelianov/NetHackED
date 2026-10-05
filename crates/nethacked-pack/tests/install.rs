use std::path::{Path, PathBuf};

use nethacked_pack::{
    build, install_pack, list_installed, nhpack_to_bytes, pack_info, resolve_installed, PackError,
};

const HARD_MODE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../packs/examples/hard-mode"
);

fn tmp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("nethacked-install-{name}"));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn install_dir_then_list_and_resolve() {
    let dir = tmp("basic");
    let inst = install_pack(Path::new(HARD_MODE), &dir, false).unwrap();
    assert!(inst.path.exists());
    assert_eq!(
        inst.path.file_name().unwrap(),
        format!("{}-{}.nhpack", inst.id, inst.version).as_str()
    );
    let listed = list_installed(&dir).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].hash, inst.hash);
    assert_eq!(resolve_installed(&dir, &inst.id).unwrap().hash, inst.hash);
    assert!(resolve_installed(&dir, "nope").is_none());
}

#[test]
fn install_same_pack_twice_is_idempotent() {
    let dir = tmp("idem");
    install_pack(Path::new(HARD_MODE), &dir, false).unwrap();
    install_pack(Path::new(HARD_MODE), &dir, false).unwrap();
    assert_eq!(list_installed(&dir).unwrap().len(), 1);
}

#[test]
fn install_refuses_hash_conflict_without_force() {
    let dir = tmp("conflict");
    let inst = install_pack(Path::new(HARD_MODE), &dir, false).unwrap();
    // Same id+version, different content: copy the pack dir and change a value.
    let src = tmp("conflict-src");
    for f in ["pack.toml", "monsters.toml", "items.toml", "roles.toml"] {
        let p = Path::new(HARD_MODE).join(f);
        if p.exists() {
            std::fs::copy(&p, src.join(f)).unwrap();
        }
    }
    let items = std::fs::read_to_string(src.join("items.toml")).unwrap_or_default();
    std::fs::write(
        src.join("items.toml"),
        format!("{items}\n[[item]]\nname = \"leather armor\"\ncost = 999\n"),
    )
    .unwrap();
    let err = install_pack(&src, &dir, false).unwrap_err();
    assert!(matches!(err, PackError::Conflict(_)), "{err}");
    let forced = install_pack(&src, &dir, true).unwrap();
    assert_ne!(forced.hash, inst.hash);
}

#[test]
fn resolve_installed_picks_highest_semver() {
    let dir = tmp("semver");
    let (mut p, _) = build(Path::new(HARD_MODE)).unwrap();
    for v in ["0.9.0", "0.10.0", "0.2.0"] {
        p.manifest.version = v.into();
        p.ruleset.manifest.version = v.into();
        p.hash = nethacked_pack::ruleset_hash(&p.ruleset);
        std::fs::write(
            dir.join(format!("{}-{v}.nhpack", p.manifest.id)),
            nhpack_to_bytes(&p),
        )
        .unwrap();
    }
    std::fs::write(dir.join("junk.nhpack"), b"not a pack").unwrap();
    let best = resolve_installed(&dir, &p.manifest.id).unwrap();
    assert_eq!(best.version, "0.10.0");
    assert_eq!(list_installed(&dir).unwrap().len(), 3);
}

#[test]
fn pack_info_reports_counts_and_diffs() {
    let info = pack_info(Path::new(HARD_MODE)).unwrap();
    assert!(info.hash.starts_with("sha256:"));
    assert!(info.monsters > 0 && info.items > 0 && info.roles > 0);
    assert!(info.diffs_vs_vanilla > 0);
}

#[test]
fn list_installed_reads_legacy_nrpack_files() {
    let dir = tmp("legacy-ext");
    let (p, _) = build(Path::new(HARD_MODE)).unwrap();
    std::fs::write(dir.join("old.nrpack"), nhpack_to_bytes(&p)).unwrap();
    let listed = list_installed(&dir).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].hash, p.hash);
}

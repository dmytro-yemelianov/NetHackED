//! In-memory pack API: same results as the path-based API.

use std::path::Path;

use nethacked_data::ruleset::Ruleset;
use nethacked_pack::{
    build, build_from_files, load_nhpack, load_nhpack_bytes, nhpack_to_bytes, parse_pack_files,
    read_pack_dir, read_pack_files_from_dir, ruleset_hash, vanilla_pack_files, write_nhpack,
    PackError, PackFiles,
};

const HARD_MODE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../packs/examples/hard-mode"
);

#[test]
fn build_from_files_matches_build_from_dir() {
    let (from_dir, _) = build(Path::new(HARD_MODE)).unwrap();
    let files = read_pack_files_from_dir(Path::new(HARD_MODE)).unwrap();
    let (from_files, _) = build_from_files(&files).unwrap();
    assert_eq!(from_dir.hash, from_files.hash);
    assert_eq!(nhpack_to_bytes(&from_dir), nhpack_to_bytes(&from_files));
}

#[test]
fn nhpack_bytes_match_write_nhpack_output() {
    let (p, _) = build(Path::new(HARD_MODE)).unwrap();
    let out = std::env::temp_dir().join("nethacked-pack-bytes-eq.nhpack");
    write_nhpack(&p, &out).unwrap();
    assert_eq!(std::fs::read(&out).unwrap(), nhpack_to_bytes(&p));
    let (_, rref_path) = load_nhpack(&out).unwrap();
    let (_, rref_bytes, _) = load_nhpack_bytes(&nhpack_to_bytes(&p)).unwrap();
    assert_eq!(rref_path, rref_bytes);
}

#[test]
fn load_nhpack_bytes_rejects_tampered_hash() {
    let (mut p, _) = build(Path::new(HARD_MODE)).unwrap();
    p.hash = "sha256:00".into();
    let err = load_nhpack_bytes(&nhpack_to_bytes(&p)).unwrap_err();
    assert!(matches!(err, PackError::Hash { .. }), "{err}");
}

#[test]
fn load_nhpack_bytes_rejects_garbage() {
    assert!(matches!(
        load_nhpack_bytes(b"{not json"),
        Err(PackError::Io(_))
    ));
    assert!(matches!(
        load_nhpack_bytes(&[0xff, 0xfe]),
        Err(PackError::Io(_))
    ));
}

#[test]
fn parse_pack_files_matches_read_pack_dir() {
    let from_dir = read_pack_dir(Path::new(HARD_MODE)).unwrap();
    let files = read_pack_files_from_dir(Path::new(HARD_MODE)).unwrap();
    let from_files = parse_pack_files(&files).unwrap();
    assert_eq!(from_dir.manifest, from_files.manifest);
    assert_eq!(from_dir.monsters, from_files.monsters);
    assert_eq!(from_dir.items, from_files.items);
    assert_eq!(from_dir.roles, from_files.roles);
    assert_eq!(from_dir.i18n, from_files.i18n);
}

#[test]
fn parse_pack_files_accepts_crlf_and_bom() {
    let files = read_pack_files_from_dir(Path::new(HARD_MODE)).unwrap();
    let mangled = PackFiles(
        files
            .0
            .iter()
            .map(|(k, v)| (k.clone(), format!("\u{feff}{}", v.replace('\n', "\r\n"))))
            .collect(),
    );
    let (a, _) = build_from_files(&files).unwrap();
    let (b, _) = build_from_files(&mangled).unwrap();
    assert_eq!(a.hash, b.hash);
}

#[test]
fn parse_pack_files_strips_common_root() {
    let files = read_pack_files_from_dir(Path::new(HARD_MODE)).unwrap();
    let nested = PackFiles(
        files
            .0
            .iter()
            .map(|(k, v)| (format!("hard-mode/{k}"), v.clone()))
            .collect(),
    );
    let (a, _) = build_from_files(&files).unwrap();
    let (b, _) = build_from_files(&nested).unwrap();
    assert_eq!(a.hash, b.hash);
}

#[test]
fn parse_pack_files_requires_manifest() {
    let err = parse_pack_files(&PackFiles::default()).unwrap_err();
    assert!(format!("{err}").contains("pack.toml"), "{err}");
}

#[test]
fn vanilla_pack_files_rebuild_to_vanilla_hash() {
    let (p, report) = build_from_files(&vanilla_pack_files()).unwrap();
    assert!(report.errors.is_empty());
    let mut vanilla = (*Ruleset::vanilla()).clone();
    vanilla.manifest = p.ruleset.manifest.clone();
    assert_eq!(p.hash, ruleset_hash(&vanilla));
}

fn with_manifest(id: &str, version: &str) -> PackFiles {
    let mut files = read_pack_files_from_dir(Path::new(HARD_MODE)).unwrap();
    files.0.insert(
        "pack.toml".into(),
        format!("id = \"{id}\"\nname = \"X\"\nversion = \"{version}\"\nbase = \"vanilla\"\n"),
    );
    files
}

#[test]
fn build_rejects_path_like_ids_and_versions() {
    for (id, version) in [
        ("../evil", "1.0.0"),
        ("/tmp/evil", "1.0.0"),
        ("a/b", "1.0.0"),
        (".hidden", "1.0.0"),
        ("", "1.0.0"),
        ("ok", "../1"),
        ("ok", "1/2"),
    ] {
        let err = build_from_files(&with_manifest(id, version)).unwrap_err();
        assert!(
            format!("{err}").contains("pack.toml"),
            "{id} {version}: {err}"
        );
    }
    assert!(build_from_files(&with_manifest("My_Pack.v2", "1.2.3-rc.1+b5")).is_ok());
}

#[test]
fn load_nhpack_bytes_rejects_mismatched_outer_manifest() {
    let (mut p, _) = build(Path::new(HARD_MODE)).unwrap();
    p.manifest.id = "someone-else".into();
    let err = load_nhpack_bytes(&nhpack_to_bytes(&p)).unwrap_err();
    assert!(format!("{err}").contains("manifest"), "{err}");
}

#[test]
fn load_nhpack_bytes_rejects_rehashed_invalid_ruleset() {
    let (mut p, _) = build(Path::new(HARD_MODE)).unwrap();
    p.ruleset.monsters.clear();
    p.hash = ruleset_hash(&p.ruleset);
    let err = load_nhpack_bytes(&nhpack_to_bytes(&p)).unwrap_err();
    assert!(matches!(err, PackError::Invalid(_)), "{err}");
}

#[test]
fn load_nhpack_bytes_rejects_path_like_id_even_when_rehashed() {
    let (mut p, _) = build(Path::new(HARD_MODE)).unwrap();
    p.manifest.id = "../evil".into();
    p.ruleset.manifest.id = "../evil".into();
    p.hash = ruleset_hash(&p.ruleset);
    assert!(load_nhpack_bytes(&nhpack_to_bytes(&p)).is_err());
}

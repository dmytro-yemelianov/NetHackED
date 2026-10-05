//! In-memory pack API: same results as the path-based API.

use std::path::Path;

use netrust_data::ruleset::Ruleset;
use netrust_pack::{
    build, build_from_files, load_nrpack, load_nrpack_bytes, nrpack_to_bytes, parse_pack_files,
    read_pack_dir, read_pack_files_from_dir, ruleset_hash, vanilla_pack_files, write_nrpack,
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
    assert_eq!(nrpack_to_bytes(&from_dir), nrpack_to_bytes(&from_files));
}

#[test]
fn nrpack_bytes_match_write_nrpack_output() {
    let (p, _) = build(Path::new(HARD_MODE)).unwrap();
    let out = std::env::temp_dir().join("netrust-pack-bytes-eq.nrpack");
    write_nrpack(&p, &out).unwrap();
    assert_eq!(std::fs::read(&out).unwrap(), nrpack_to_bytes(&p));
    let (_, rref_path) = load_nrpack(&out).unwrap();
    let (_, rref_bytes, _) = load_nrpack_bytes(&nrpack_to_bytes(&p)).unwrap();
    assert_eq!(rref_path, rref_bytes);
}

#[test]
fn load_nrpack_bytes_rejects_tampered_hash() {
    let (mut p, _) = build(Path::new(HARD_MODE)).unwrap();
    p.hash = "sha256:00".into();
    let err = load_nrpack_bytes(&nrpack_to_bytes(&p)).unwrap_err();
    assert!(matches!(err, PackError::Hash { .. }), "{err}");
}

#[test]
fn load_nrpack_bytes_rejects_garbage() {
    assert!(matches!(
        load_nrpack_bytes(b"{not json"),
        Err(PackError::Io(_))
    ));
    assert!(matches!(
        load_nrpack_bytes(&[0xff, 0xfe]),
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

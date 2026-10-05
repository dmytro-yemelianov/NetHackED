//! Integration tests for the nethacked-pack CLI binary.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use nethacked_data::ruleset::Ruleset;
use nethacked_pack::load_nhpack;

fn bin_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_nethacked-pack"))
}

fn temp_cli_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nethacked-pack-cli-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn test_cli_new_and_build() {
    let dir = temp_cli_dir("new-and-build");
    let pack_dir = dir.join("my-pack");

    // 1. `new` happy path
    let output = Command::new(bin_path())
        .args([
            "new",
            pack_dir.to_str().unwrap(),
            "--id",
            "custom-pack",
            "--name",
            "Custom Pack",
        ])
        .output()
        .expect("failed to execute nethacked-pack");
    assert!(
        output.status.success(),
        "new command failed: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(pack_dir.join("pack.toml").exists());
    assert!(pack_dir.join("monsters.toml").exists());
    assert!(pack_dir.join("items.toml").exists());
    assert!(pack_dir.join("roles.toml").exists());

    // 2. Refuses non-empty dir
    let output_err = Command::new(bin_path())
        .args(["new", pack_dir.to_str().unwrap()])
        .output()
        .expect("failed to execute nethacked-pack");
    assert!(!output_err.status.success());
    let stderr = String::from_utf8_lossy(&output_err.stderr);
    assert!(stderr.contains("not empty"));

    // 3. `validate` on fresh skeleton directory
    let val_out = Command::new(bin_path())
        .args(["validate", pack_dir.to_str().unwrap()])
        .output()
        .expect("failed to validate");
    assert!(
        val_out.status.success(),
        "validation failed: {:?}",
        String::from_utf8_lossy(&val_out.stderr)
    );

    // 4. `build` into .nhpack
    let nhpack_file = dir.join("custom.nhpack");
    let build_out = Command::new(bin_path())
        .args([
            "build",
            pack_dir.to_str().unwrap(),
            "-o",
            nhpack_file.to_str().unwrap(),
        ])
        .output()
        .expect("failed to build");
    assert!(
        build_out.status.success(),
        "build failed: {:?}",
        String::from_utf8_lossy(&build_out.stderr)
    );
    let hash = String::from_utf8_lossy(&build_out.stdout)
        .trim()
        .to_string();
    assert!(hash.starts_with("sha256:"));
    assert!(nhpack_file.exists());

    // 5. `validate` on built .nhpack file
    let val_file_out = Command::new(bin_path())
        .args(["validate", nhpack_file.to_str().unwrap()])
        .output()
        .expect("failed to validate file");
    assert!(
        val_file_out.status.success(),
        "nhpack validation failed: {:?}",
        String::from_utf8_lossy(&val_file_out.stderr)
    );
}

#[test]
fn test_cli_validate_broken_pack() {
    let dir = temp_cli_dir("broken-pack");
    let pack_dir = dir.join("broken");

    Command::new(bin_path())
        .args(["new", pack_dir.to_str().unwrap()])
        .output()
        .expect("failed to run new");

    // Inject out-of-range level
    let mut monsters = fs::read_to_string(pack_dir.join("monsters.toml")).unwrap();
    monsters.push_str("\n[[monster]]\nname = \"jackal\"\nlevel = 99\n");
    fs::write(pack_dir.join("monsters.toml"), monsters).unwrap();

    let val_out = Command::new(bin_path())
        .args(["validate", pack_dir.to_str().unwrap()])
        .output()
        .expect("failed to validate");

    assert!(!val_out.status.success());
    let err_msg = format!(
        "{}\n{}",
        String::from_utf8_lossy(&val_out.stdout),
        String::from_utf8_lossy(&val_out.stderr)
    );
    assert!(
        err_msg.contains("level"),
        "expected level in error, got {err_msg}"
    );
    assert!(
        err_msg.contains("out of range"),
        "expected out of range, got {err_msg}"
    );
}

#[test]
fn test_cli_export_vanilla_and_equality() {
    let dir = temp_cli_dir("export-vanilla");
    let export_dir = dir.join("vanilla-pack");

    // 1. Export vanilla
    let export_out = Command::new(bin_path())
        .args(["export-vanilla", export_dir.to_str().unwrap()])
        .output()
        .expect("failed to export vanilla");
    assert!(
        export_out.status.success(),
        "export-vanilla failed: {:?}",
        String::from_utf8_lossy(&export_out.stderr)
    );
    assert!(export_dir.join("pack.toml").exists());
    assert!(export_dir.join("monsters.toml").exists());

    // 2. Build exported vanilla
    let nhpack_file = dir.join("vanilla-export.nhpack");
    let build_out = Command::new(bin_path())
        .args([
            "build",
            export_dir.to_str().unwrap(),
            "-o",
            nhpack_file.to_str().unwrap(),
        ])
        .output()
        .expect("failed to build exported vanilla");
    assert!(
        build_out.status.success(),
        "build exported vanilla failed: {:?}",
        String::from_utf8_lossy(&build_out.stderr)
    );

    // 3. Verify resolved ruleset data equals Ruleset::vanilla()
    let (loaded_rs, _) = load_nhpack(&nhpack_file).expect("must load exported nhpack");
    let vanilla = Ruleset::vanilla();

    assert_eq!(loaded_rs.monsters.len(), vanilla.monsters.len());
    assert_eq!(loaded_rs.monsters, vanilla.monsters);
    assert_eq!(loaded_rs.items.len(), vanilla.items.len());
    assert_eq!(loaded_rs.items, vanilla.items);
    assert_eq!(loaded_rs.roles.len(), vanilla.roles.len());
    assert_eq!(loaded_rs.roles, vanilla.roles);
    assert_eq!(loaded_rs.races, vanilla.races);
    assert_eq!(loaded_rs.manifest.id, "vanilla-export");
}

#[test]
fn test_cli_diff() {
    // 1. Diff vanilla vanilla -> empty, exit code 0
    let diff_out = Command::new(bin_path())
        .args(["diff", "vanilla", "vanilla"])
        .output()
        .expect("failed to diff");
    assert!(diff_out.status.success());
    let stdout = String::from_utf8_lossy(&diff_out.stdout);
    assert!(
        stdout.trim().is_empty(),
        "expected empty diff, got {stdout}"
    );

    // 2. Diff vanilla with patched pack
    let dir = temp_cli_dir("diff-cli");
    let pack_dir = dir.join("diff-pack");
    Command::new(bin_path())
        .args(["new", pack_dir.to_str().unwrap()])
        .output()
        .expect("failed to run new");

    fs::write(
        pack_dir.join("monsters.toml"),
        "[[monster]]\nname = \"jackal\"\nlevel = 4\n",
    )
    .unwrap();

    let diff_pack_out = Command::new(bin_path())
        .args(["diff", "vanilla", pack_dir.to_str().unwrap()])
        .output()
        .expect("failed to diff against pack");
    assert!(diff_pack_out.status.success());
    let diff_str = String::from_utf8_lossy(&diff_pack_out.stdout);
    assert!(diff_str.contains("monsters jackal level:"));
    assert!(diff_str.contains("-> 4"));
}

#[test]
fn test_cli_simulate() {
    // 1. Table output
    let sim_out = Command::new(bin_path())
        .args(["simulate", "vanilla", "--seeds", "1", "--turns", "50"])
        .output()
        .expect("failed to simulate");
    assert!(
        sim_out.status.success(),
        "simulate failed: {:?}",
        String::from_utf8_lossy(&sim_out.stderr)
    );
    let stdout = String::from_utf8_lossy(&sim_out.stdout);
    assert!(stdout.contains("Zero panics encountered"));

    // 2. JSON output
    let sim_json_out = Command::new(bin_path())
        .args([
            "simulate", "vanilla", "--seeds", "1", "--turns", "50", "--json",
        ])
        .output()
        .expect("failed to simulate json");
    assert!(sim_json_out.status.success());
    let json_val: serde_json::Value =
        serde_json::from_slice(&sim_json_out.stdout).expect("output must be valid json");
    assert_eq!(json_val["total_panics"], 0);
    assert_eq!(json_val["seeds"], 1);
}

#[test]
fn test_cli_schema() {
    let dir = temp_cli_dir("schema-cli");
    let schema_dir = dir.join("schemas");

    let out = Command::new(bin_path())
        .args(["schema", "-o", schema_dir.to_str().unwrap()])
        .output()
        .expect("failed to run schema");
    assert!(
        out.status.success(),
        "schema failed: {:?}",
        String::from_utf8_lossy(&out.stderr)
    );

    let files = [
        "pack.schema.json",
        "monsters.schema.json",
        "items.schema.json",
        "roles.schema.json",
    ];

    for file in files {
        let path = schema_dir.join(file);
        assert!(path.exists(), "schema file {file} must exist");
        let content = fs::read_to_string(&path).unwrap();
        let _: serde_json::Value =
            serde_json::from_str(&content).expect("schema file must be valid json");
    }
}

#[test]
fn cli_install_list_info_roundtrip() {
    let dir = temp_cli_dir("install-roundtrip");
    let hard = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../packs/examples/hard-mode"
    );
    let out = Command::new(bin_path())
        .args(["install", hard, "--dir"])
        .arg(&dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let out = Command::new(bin_path())
        .args(["list", "--dir"])
        .arg(&dir)
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("sha256:"), "{stdout}");
    let out = Command::new(bin_path())
        .args(["info", hard])
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&out.stdout).contains("diffs vs vanilla:"));
}

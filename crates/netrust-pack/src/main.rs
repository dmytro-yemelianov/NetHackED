//! netrust-pack command-line interface.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use netrust_data::ruleset::{Ruleset, RulesetRef};
use netrust_pack::{
    build, diff, load_nrpack, read_pack_dir, resolve, run_simulation, validate, write_nrpack,
    ItemsToml, MonstersToml, PackError, PackToml, RolesToml,
};

#[derive(Parser)]
#[command(name = "netrust-pack", about = "NetRust Rule Pack Tooling", version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new rule pack skeleton in the target directory
    New {
        dir: PathBuf,
        #[arg(long)]
        id: Option<String>,
        #[arg(long)]
        name: Option<String>,
    },
    /// Validate a rule pack directory or compiled .nrpack file
    Validate { path: PathBuf },
    /// Build a rule pack directory into a .nrpack file
    Build {
        dir: PathBuf,
        #[arg(short, long)]
        out: PathBuf,
    },
    /// Compare two rulesets (vanilla, pack directory, or .nrpack)
    Diff { a: String, b: String },
    /// Export the vanilla ruleset into declarative TOML patch files
    ExportVanilla { dir: PathBuf },
    /// Generate JSON schemas for pack configuration files
    Schema {
        #[arg(short, long)]
        out: PathBuf,
    },
    /// Simulate gameplay comparing a rule pack against vanilla
    Simulate {
        pack: String,
        #[arg(long, default_value_t = 5)]
        seeds: u64,
        #[arg(long, default_value_t = 500)]
        turns: u64,
        #[arg(long)]
        json: bool,
    },
}

fn load_source(source: &str) -> Result<(Arc<Ruleset>, RulesetRef), String> {
    if source == "vanilla" {
        return Ok((Ruleset::vanilla(), RulesetRef::vanilla()));
    }
    let p = Path::new(source);
    if p.is_file() {
        load_nrpack(p).map_err(|e| format!("{e}"))
    } else if p.is_dir() {
        let (nrpack, _) = build(p).map_err(|e| format!("{e}"))?;
        let rref = RulesetRef {
            id: nrpack.manifest.id,
            version: nrpack.manifest.version,
            hash: nrpack.hash,
        };
        Ok((Arc::new(nrpack.ruleset), rref))
    } else {
        Err(format!(
            "source '{source}' does not exist as file or directory"
        ))
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();

    match cli.command {
        Commands::New { dir, id, name } => {
            if dir.exists() {
                match fs::read_dir(&dir) {
                    Ok(mut entries) => {
                        if entries.next().is_some() {
                            eprintln!("Error: Directory '{}' is not empty", dir.display());
                            return ExitCode::FAILURE;
                        }
                    }
                    Err(e) => {
                        eprintln!("Error accessing '{}': {e}", dir.display());
                        return ExitCode::FAILURE;
                    }
                }
            } else if let Err(e) = fs::create_dir_all(&dir) {
                eprintln!("Error creating directory '{}': {e}", dir.display());
                return ExitCode::FAILURE;
            }

            let pack_id = id.unwrap_or_else(|| {
                dir.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("custom-pack")
                    .to_string()
            });

            let pack_name = name.unwrap_or_else(|| {
                pack_id
                    .split('-')
                    .map(|w| {
                        let mut chars = w.chars();
                        match chars.next() {
                            Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                            None => String::new(),
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            });

            let pack_toml = format!(
                r#"id = "{pack_id}"
name = "{pack_name}"
version = "0.1.0"
base = "vanilla"
description = "A custom NetRust rule pack"
"#
            );

            let monsters_toml = r#"# Monsters configuration for NetRust
# Example: Patch an existing monster
# [[monster]]
# name = "jackal"
# level = 3

# Example: Add a new monster
# [[monster]]
# name = "dire jackal"
# new = true
# glyph = "d"
# base_hp = 10
# max_hp = 10
# ac = 5
# level = 2
# speed = 14
# alignment = "Neutral"
# intrinsics = {}
# attacks = [{ at = "Bite", ad = "Phys", n = 2, d = 4 }]
# size = "Medium"
# peaceful_by_default = false
# always_hostile = true
# maligntyp = 0
# msound = "Other"
# is_human = false
# is_unique = false
# mindless = false
# ai_behavior = "MeleeHunter"
# abilities = []
"#;

            let items_toml = r#"# Items configuration for NetRust
# Example: Patch an existing item
# [[item]]
# name = "leather armor"
# cost = 10
"#;

            let roles_toml = r#"# Roles configuration for NetRust
# Example: Patch an existing role
# [[role]]
# name = "Valkyrie"
# base_hp = 20
"#;

            if let Err(e) = fs::write(dir.join("pack.toml"), pack_toml) {
                eprintln!("Error writing pack.toml: {e}");
                return ExitCode::FAILURE;
            }
            if let Err(e) = fs::write(dir.join("monsters.toml"), monsters_toml) {
                eprintln!("Error writing monsters.toml: {e}");
                return ExitCode::FAILURE;
            }
            if let Err(e) = fs::write(dir.join("items.toml"), items_toml) {
                eprintln!("Error writing items.toml: {e}");
                return ExitCode::FAILURE;
            }
            if let Err(e) = fs::write(dir.join("roles.toml"), roles_toml) {
                eprintln!("Error writing roles.toml: {e}");
                return ExitCode::FAILURE;
            }

            println!("Created new rule pack skeleton at '{}'", dir.display());
            ExitCode::SUCCESS
        }

        Commands::Validate { path } => {
            if path.is_file() {
                match load_nrpack(&path) {
                    Ok((_, rref)) => {
                        println!(
                            "Validation passed: {} is valid (hash: {})",
                            path.display(),
                            rref.hash
                        );
                        ExitCode::SUCCESS
                    }
                    Err(e) => {
                        eprintln!("error: {}: {e}", path.display());
                        ExitCode::FAILURE
                    }
                }
            } else if path.is_dir() {
                let pack = match read_pack_dir(&path) {
                    Ok(p) => p,
                    Err(PackError::Toml { file, message }) => {
                        eprintln!("error: {file}: {message}");
                        return ExitCode::FAILURE;
                    }
                    Err(PackError::Resolve(diag)) => {
                        eprintln!(
                            "error: {}: {}{}: {}",
                            diag.file,
                            diag.entry,
                            diag.field
                                .as_deref()
                                .map(|f| format!(".{f}"))
                                .unwrap_or_default(),
                            diag.message
                        );
                        return ExitCode::FAILURE;
                    }
                    Err(e) => {
                        eprintln!("error: {}: {e}", path.display());
                        return ExitCode::FAILURE;
                    }
                };

                let ruleset = match resolve(&pack, &Ruleset::vanilla()) {
                    Ok(rs) => rs,
                    Err(PackError::Resolve(diag)) => {
                        eprintln!(
                            "error: {}: {}{}: {}",
                            diag.file,
                            diag.entry,
                            diag.field
                                .as_deref()
                                .map(|f| format!(".{f}"))
                                .unwrap_or_default(),
                            diag.message
                        );
                        return ExitCode::FAILURE;
                    }
                    Err(e) => {
                        eprintln!("error: {e}");
                        return ExitCode::FAILURE;
                    }
                };

                let report = validate(&ruleset);

                for d in &report.errors {
                    eprintln!(
                        "error: {}: {}{}: {}",
                        d.file,
                        d.entry,
                        d.field
                            .as_deref()
                            .map(|f| format!(".{f}"))
                            .unwrap_or_default(),
                        d.message
                    );
                }

                for d in &report.warnings {
                    eprintln!(
                        "warning: {}: {}{}: {}",
                        d.file,
                        d.entry,
                        d.field
                            .as_deref()
                            .map(|f| format!(".{f}"))
                            .unwrap_or_default(),
                        d.message
                    );
                }

                if report.has_errors() {
                    eprintln!(
                        "Validation failed with {} error(s) and {} warning(s).",
                        report.errors.len(),
                        report.warnings.len()
                    );
                    ExitCode::FAILURE
                } else {
                    println!(
                        "Validation passed with 0 error(s) and {} warning(s).",
                        report.warnings.len()
                    );
                    ExitCode::SUCCESS
                }
            } else {
                eprintln!("error: Path '{}' not found", path.display());
                ExitCode::FAILURE
            }
        }

        Commands::Build { dir, out } => match build(&dir) {
            Ok((nrpack, report)) => {
                for d in &report.warnings {
                    eprintln!(
                        "warning: {}: {}{}: {}",
                        d.file,
                        d.entry,
                        d.field
                            .as_deref()
                            .map(|f| format!(".{f}"))
                            .unwrap_or_default(),
                        d.message
                    );
                }
                if let Err(e) = write_nrpack(&nrpack, &out) {
                    eprintln!("error writing nrpack to '{}': {e}", out.display());
                    return ExitCode::FAILURE;
                }
                println!("{}", nrpack.hash);
                ExitCode::SUCCESS
            }
            Err(PackError::Invalid(report)) => {
                for d in &report.errors {
                    eprintln!(
                        "error: {}: {}{}: {}",
                        d.file,
                        d.entry,
                        d.field
                            .as_deref()
                            .map(|f| format!(".{f}"))
                            .unwrap_or_default(),
                        d.message
                    );
                }
                for d in &report.warnings {
                    eprintln!(
                        "warning: {}: {}{}: {}",
                        d.file,
                        d.entry,
                        d.field
                            .as_deref()
                            .map(|f| format!(".{f}"))
                            .unwrap_or_default(),
                        d.message
                    );
                }
                ExitCode::FAILURE
            }
            Err(e) => {
                eprintln!("error: {e}");
                ExitCode::FAILURE
            }
        },

        Commands::Diff { a, b } => {
            let rs_a = match load_source(&a) {
                Ok((rs, _)) => rs,
                Err(e) => {
                    eprintln!("error: {e}");
                    return ExitCode::FAILURE;
                }
            };
            let rs_b = match load_source(&b) {
                Ok((rs, _)) => rs,
                Err(e) => {
                    eprintln!("error: {e}");
                    return ExitCode::FAILURE;
                }
            };

            let diffs = diff(&rs_a, &rs_b);
            for d in diffs {
                println!(
                    "{} {} {}: {} -> {}",
                    d.table, d.entry, d.field, d.before, d.after
                );
            }
            ExitCode::SUCCESS
        }

        Commands::ExportVanilla { dir } => {
            if let Err(e) = fs::create_dir_all(&dir) {
                eprintln!("error creating directory '{}': {e}", dir.display());
                return ExitCode::FAILURE;
            }

            for (name, text) in netrust_pack::vanilla_pack_files().0 {
                if let Err(e) = fs::write(dir.join(&name), text) {
                    eprintln!("error writing {name}: {e}");
                    return ExitCode::FAILURE;
                }
            }

            println!("Exported vanilla ruleset to '{}'", dir.display());
            ExitCode::SUCCESS
        }

        Commands::Schema { out } => {
            #[cfg(feature = "schema")]
            {
                if let Err(e) = fs::create_dir_all(&out) {
                    eprintln!("error creating directory '{}': {e}", out.display());
                    return ExitCode::FAILURE;
                }

                let pack_schema = schemars::schema_for!(PackToml);
                let monsters_schema = schemars::schema_for!(MonstersToml);
                let items_schema = schemars::schema_for!(ItemsToml);
                let roles_schema = schemars::schema_for!(RolesToml);

                let files = [
                    ("pack.schema.json", pack_schema),
                    ("monsters.schema.json", monsters_schema),
                    ("items.schema.json", items_schema),
                    ("roles.schema.json", roles_schema),
                ];

                for (name, schema) in files {
                    let content = match serde_json::to_string_pretty(&schema) {
                        Ok(c) => c,
                        Err(e) => {
                            eprintln!("error serializing schema {name}: {e}");
                            return ExitCode::FAILURE;
                        }
                    };
                    if let Err(e) = fs::write(out.join(name), content) {
                        eprintln!("error writing {name}: {e}");
                        return ExitCode::FAILURE;
                    }
                }

                println!("Wrote JSON schemas to '{}'", out.display());
                ExitCode::SUCCESS
            }
            #[cfg(not(feature = "schema"))]
            {
                eprintln!("error: schema generation requires the 'schema' feature");
                ExitCode::FAILURE
            }
        }

        Commands::Simulate {
            pack,
            seeds,
            turns,
            json,
        } => {
            let (rs, rref) = match load_source(&pack) {
                Ok(res) => res,
                Err(e) => {
                    eprintln!("error loading pack: {e}");
                    return ExitCode::FAILURE;
                }
            };

            let report = run_simulation(rs, rref, seeds, turns);

            if json {
                println!("{}", serde_json::to_string_pretty(&report).unwrap());
            } else {
                println!(
                    "Simulation results ({} seeds, {} turns/game):",
                    report.seeds, report.turns_per_game
                );
                println!(
                    "{:<14} | {:<32} | {:<32}",
                    "Role", "Pack (G / D / Depth / Turns)", "Vanilla (G / D / Depth / Turns)"
                );
                println!("{:-<14}-|-{:-<32}-|-{:-<32}", "", "", "");

                for (p, v) in report.pack_stats.iter().zip(&report.vanilla_stats) {
                    let p_str = format!(
                        "{}/{} (d:{:.1}, t:{:.0}, p:{})",
                        p.games,
                        p.deaths,
                        p.mean_depth(),
                        p.mean_turns(),
                        p.panics
                    );
                    let v_str = format!(
                        "{}/{} (d:{:.1}, t:{:.0}, p:{})",
                        v.games,
                        v.deaths,
                        v.mean_depth(),
                        v.mean_turns(),
                        v.panics
                    );
                    println!("{:<14} | {:<32} | {:<32}", p.role, p_str, v_str);
                }
                println!();
                if report.total_panics > 0 {
                    eprintln!("WARNING: Encountered {} panic(s)!", report.total_panics);
                } else {
                    println!("Zero panics encountered across all runs.");
                }
            }

            if report.total_panics > 0 {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }
    }
}

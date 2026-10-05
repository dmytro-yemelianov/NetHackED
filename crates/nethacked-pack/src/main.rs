//! nethacked-pack command-line interface.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use clap::{Parser, Subcommand};
use nethacked_data::ruleset::{Ruleset, RulesetRef};
use nethacked_pack::{
    build, diff, load_nhpack, read_pack_dir, resolve, run_simulation, validate, write_nhpack,
    ItemsToml, MonstersToml, PackError, PackToml, RolesToml,
};

#[derive(Parser)]
#[command(
    name = "nethacked-pack",
    about = "NetHackED Rule Pack Tooling",
    version
)]
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
    /// Validate a rule pack directory or compiled .nhpack file
    Validate { path: PathBuf },
    /// Build a rule pack directory into a .nhpack file
    Build {
        dir: PathBuf,
        #[arg(short, long)]
        out: PathBuf,
    },
    /// Compare two rulesets (vanilla, pack directory, or .nhpack)
    Diff { a: String, b: String },
    /// Export the vanilla ruleset into declarative TOML patch files
    ExportVanilla { dir: PathBuf },
    /// Generate JSON schemas for pack configuration files
    Schema {
        #[arg(short, long)]
        out: PathBuf,
    },
    /// List rule packs installed in the local packs directory
    List {
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Show manifest, hash, counts and vanilla diff size of a pack (path or installed id)
    Info { pack: String },
    /// Validate/build a pack and copy it into the local packs directory
    Install {
        path: PathBuf,
        #[arg(long)]
        dir: Option<PathBuf>,
        #[arg(long)]
        force: bool,
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
        load_nhpack(p).map_err(|e| format!("{e}"))
    } else if p.is_dir() {
        let (nhpack, _) = build(p).map_err(|e| format!("{e}"))?;
        let rref = RulesetRef {
            id: nhpack.manifest.id,
            version: nhpack.manifest.version,
            hash: nhpack.hash,
        };
        Ok((Arc::new(nhpack.ruleset), rref))
    } else {
        Err(format!(
            "source '{source}' does not exist as file or directory"
        ))
    }
}

fn packs_dir_or_exit(dir: Option<PathBuf>) -> Result<PathBuf, ExitCode> {
    dir.or_else(nethacked_pack::default_packs_dir)
        .ok_or_else(|| {
            eprintln!("error: no packs directory (set NETHACKED_PACKS_DIR or HOME, or pass --dir)");
            ExitCode::FAILURE
        })
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
description = "A custom NetHackED rule pack"
"#
            );

            let monsters_toml = r#"# Monsters configuration for NetHackED
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

            let items_toml = r#"# Items configuration for NetHackED
# Example: Patch an existing item
# [[item]]
# name = "leather armor"
# cost = 10
"#;

            let roles_toml = r#"# Roles configuration for NetHackED
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
                match load_nhpack(&path) {
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
            Ok((nhpack, report)) => {
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
                if let Err(e) = write_nhpack(&nhpack, &out) {
                    eprintln!("error writing nhpack to '{}': {e}", out.display());
                    return ExitCode::FAILURE;
                }
                println!("{}", nhpack.hash);
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

            for (name, text) in nethacked_pack::vanilla_pack_files().0 {
                if let Err(e) = fs::write(dir.join(&name), text) {
                    eprintln!("error writing {name}: {e}");
                    return ExitCode::FAILURE;
                }
            }

            println!("Exported vanilla ruleset to '{}'", dir.display());
            ExitCode::SUCCESS
        }

        Commands::List { dir } => {
            let dir = match packs_dir_or_exit(dir) {
                Ok(d) => d,
                Err(c) => return c,
            };
            match nethacked_pack::list_installed(&dir) {
                Ok(list) if list.is_empty() => {
                    println!("No packs installed in '{}'", dir.display());
                    ExitCode::SUCCESS
                }
                Ok(list) => {
                    for p in list {
                        println!("{}\t{}\t{}\t{}", p.id, p.version, p.hash, p.path.display());
                    }
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    ExitCode::FAILURE
                }
            }
        }

        Commands::Info { pack } => {
            let p = Path::new(&pack);
            let src = if p.exists() {
                p.to_path_buf()
            } else {
                match nethacked_pack::default_packs_dir()
                    .and_then(|d| nethacked_pack::resolve_installed(&d, &pack))
                {
                    Some(i) => i.path,
                    None => {
                        eprintln!("error: pack '{pack}' not found");
                        return ExitCode::FAILURE;
                    }
                }
            };
            match nethacked_pack::pack_info(&src) {
                Ok(i) => {
                    println!("id:       {}", i.manifest.id);
                    println!("name:     {}", i.manifest.name);
                    println!("version:  {}", i.manifest.version);
                    println!("hash:     {}", i.hash);
                    println!(
                        "monsters: {}  items: {}  roles: {}",
                        i.monsters, i.items, i.roles
                    );
                    println!("warnings: {}", i.warnings);
                    println!("diffs vs vanilla: {}", i.diffs_vs_vanilla);
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    ExitCode::FAILURE
                }
            }
        }

        Commands::Install { path, dir, force } => {
            let dir = match packs_dir_or_exit(dir) {
                Ok(d) => d,
                Err(c) => return c,
            };
            match nethacked_pack::install_pack(&path, &dir, force) {
                Ok(i) => {
                    println!(
                        "Installed {} {} ({}) -> {}",
                        i.id,
                        i.version,
                        i.hash,
                        i.path.display()
                    );
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    ExitCode::FAILURE
                }
            }
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

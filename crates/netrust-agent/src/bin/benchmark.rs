//! Automated AI Benchmark CLI for NetRust.
//!
//! Executes multi-agent seeded benchmark evaluation suites, prints leaderboard
//! metrics, and exports structured JSON reports.

use netrust_agent::arena::{run_evaluation_suite, BenchmarkReport};
use netrust_data::roles::RoleId;
use std::fs::File;
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("====================================================================================================");
    println!("                        NETRUST AUTOMATED AI BENCHMARK & EVALUATION ARENA                           ");
    println!("                        Formally Verified NetHack Simulation Engine                                 ");
    println!("====================================================================================================");

    let seeds: Vec<u64> = (1..=25).collect();
    let roles = vec![RoleId::Valkyrie, RoleId::Wizard];
    let max_turns = 1000u64;

    let total_expected = seeds.len() * roles.len() * 4;
    println!(
        "Executing {} seeded simulation runs across {} seeds and {} roles...",
        total_expected,
        seeds.len(),
        roles.len()
    );

    let (runs, summary) = run_evaluation_suite(&seeds, &roles, max_turns);

    println!("\n+--------------------+------+------+----------+------------+------------+------------+-----------+");
    println!("| POLICY             | RUNS | WINS | WIN RATE | MEAN TURNS | MEAN DEPTH | MEAN KILLS | MEAN GOLD |");
    println!("+--------------------+------+------+----------+------------+------------+------------+-----------+");

    let mut policy_names: Vec<_> = summary.per_policy.keys().cloned().collect();
    policy_names.sort();

    for name in &policy_names {
        let stats = &summary.per_policy[name];
        println!(
            "| {:<18} | {:>4} | {:>4} | {:>7.1}% | {:>10.1} | {:>10.2} | {:>10.2} | {:>9.1} |",
            name,
            stats.runs,
            stats.victories,
            stats.win_rate_pct,
            stats.mean_turns,
            stats.mean_max_depth,
            stats.mean_kills,
            stats.mean_gold
        );
    }
    println!("+--------------------+------+------+----------+------------+------------+------------+-----------+");
    println!(
        "Overall: {} runs, {} victories ({:.1}% win rate)\n",
        summary.total_runs, summary.total_victories, summary.overall_win_rate_pct
    );

    // Save JSON benchmark report
    let timestamp_epoch_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let report = BenchmarkReport {
        timestamp_epoch_secs,
        total_runs: summary.total_runs,
        summary,
        runs,
    };

    let report_path = "web/benchmark_report.json";
    let json_bytes = serde_json::to_string_pretty(&report)?;
    let mut file = File::create(report_path)?;
    file.write_all(json_bytes.as_bytes())?;

    println!("Benchmark report successfully written to `{report_path}`.");
    println!("====================================================================================================");

    Ok(())
}

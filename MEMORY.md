# MEMORY.md - Durable Facts and Decisions

## Architecture & System Context
- **NetRust Architecture**: Roguelike engine written in Rust with deterministic simulation, arena entity storage, NetHack 3.6.1 fidelity, TUI, WASM bindings, Python bindings (`netrust-py`), and Lean 4 formal proofs.
- **Rule Packs P1**: Transitioning from hardcoded static tables (`BESTIARY`, `ITEM_CATALOG`, `ROLES`, `RACES`) to an owned `Arc<Ruleset>` loaded at world init, while preserving bit-for-bit vanilla determinism.
- **Task 1 Baseline**: Completed in commits `7a785f6` and `055487c`. Golden determinism tests freeze arena runs (`EXPECTED_RUNS`), scripted event logs (`EXPECTED_LOGS`), and bestiary fights against all species for 3 roles.
- **NeMo Agent Toolkit (`nat`)**: Installed at `.venv` (`nvidia-nat==1.9.0`) with custom extension package `netrust-nat` (`nat/`) registering 4 tools: `netrust_workspace_reader`, `netrust_cargo_runner`, `netrust_golden_verifier`, `netrust_rulepacks_auditor`.

## Rules & Constraints
- Branch: `feat/rule-packs-p1`. Never commit to `main`.
- Canonical JSON = `serde_json::to_vec(&serde_json::to_value(x)?)` with sorted keys (serde_json default BTreeMap map, no `preserve_order`).
- `netrust-wasm` must never include `toml`, `clap`, `schemars`, or `netrust-pack`.
- Commit trailer required:
  ```
  Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
  Claude-Session: https://claude.ai/code/session_019cXhwXQqEEChd362EQ6RKp
  ```

# MEMORY.md - Durable Facts and Decisions

## Architecture & System Context
- **NetHackED Architecture**: Roguelike engine written in Rust with deterministic simulation, arena entity storage, NetHack 3.6.1 fidelity, TUI, WASM bindings, Python bindings (`nethacked-py`), and Lean 4 formal proofs.
- **Rule Packs P1**: Transitioning from hardcoded static tables (`BESTIARY`, `ITEM_CATALOG`, `ROLES`, `RACES`) to an owned `Arc<Ruleset>` loaded at world init, while preserving bit-for-bit vanilla determinism.
- **Task 1 Baseline**: Completed in commits `7a785f6` and `055487c`. Golden determinism tests freeze arena runs (`EXPECTED_RUNS`), scripted event logs (`EXPECTED_LOGS`), and bestiary fights against all species for 3 roles.
- **NeMo Agent Toolkit (`nat`)**: Installed at `.venv` (`nvidia-nat==1.9.0`) with custom extension package `nethacked-nat` (`nat/`) registering 4 tools: `nethacked_workspace_reader`, `nethacked_cargo_runner`, `nethacked_golden_verifier`, `nethacked_rulepacks_auditor`.

## Rules & Constraints
- **2026-10-06 owned room item selection delivered**: ordinary rooms select owned definitions/weights and retain custom items without built-in IDs. Canonical built-in order and stable custom order preserve vanilla draws; all770tests/all3unchangedgoldens/all12gates pass, Sol/high review without defects. Legacy missing generation weights retain omitted encoding/hash and owned compatibility probabilities; new explicit zero disables and round-trips as zero. New vanilla canonical JSON emits zero weights explicitly. Container selection unchanged; no dependencies.
- **2026-10-06 item initialization delivered**: I1 BUC/enchantment/charges and weapon quantity/poison (`85eed17`), I2a ordinary owned containers (`0eaaa87`), I2b typed ice-box corpses and once-per-world-turn active/cached aging (`ecc14b9`), then I3 existing-object random artifacts with canonical identity and persistent creation history. Sequential Sol/medium implementation and independent Sol/high review; final762tests/all3goldens/all12gates pass. Verified intentional RUNS rebaselines authorized; LOGS/COMBAT unchanged during item work. Full C corpse gender/revival/glob/rot deletion, other food species variants/fuel/erosion extras and artifact gift/power contexts remain gaps.
- **2026-10-06 item-init policy (explicit user decision)**: affected goldens may be rebaselined for verified intentional item values/RNG changes after independent review. Coordinator owns checked updates; unrelated changes remain frozen. Execute the item-init contract in order with full gates before commits.
- **2026-10-06 review repairs completed**: T2 (`aefca8f`) gives core ownership of all transient countdowns and sim emits recovery transitions once; fatal ticks preserve core early returns. T3 spawns randomly selected custom definitions by name and retains generation/genocide filtering. Independent reviews and full final gates passed (700 tests, all three goldens unchanged after T1); no dependency changes. Scoped item initialization is delivered in the later I1–I3 units; full C `mksobj` fidelity remains partial.
- **2026-10-06 HP policy (explicit user decision)**: vanilla monsters sample HP using C `newmonhp` formulas and the owned Ruleset/world RNG. Explicit pack HP overrides and custom definitions stay fixed by default; `sample_hp` controls opt-in/out. Bones ghosts keep supplied HP without a discarded roll. Sampling uses definition level (`adj_lev` remains unmodelled); Astral context is integrated, other planes sampler-only.
- **2026-10-06 approved HP rebaseline**: `EXPECTED_RUNS` and `EXPECTED_COMBAT` changed after independent verification; `EXPECTED_LOGS` stayed identical. Full gate and all three golden tests passed (695 workspace tests). This is an approved intentional HP/RNG change, not permission for unrelated automatic rebaselines. T1 committed as `049ed98` after user authorization; follow-up verification is recorded in the execution plan.
- Original P1 branch: `feat/rule-packs-p1`; current authorized repair branch: `feat/monster-hp-d8`. Never commit to `main`.
- Canonical JSON = `serde_json::to_vec(&serde_json::to_value(x)?)` with sorted keys (serde_json default BTreeMap map, no `preserve_order`).
- `nethacked-wasm` must never include `toml`, `clap`, `schemars`, or `nethacked-pack`.
- Commit trailer required:
  ```
  Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
  Claude-Session: https://claude.ai/code/session_019cXhwXQqEEChd362EQ6RKp
  ```

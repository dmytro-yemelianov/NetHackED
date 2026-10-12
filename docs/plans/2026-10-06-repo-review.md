# Repository review — 2026-10-06

Scope: current working tree on `feat/monster-hp-d8`, including uncommitted changes. Review only; preserve existing edits and golden expectations; no commit.

1. Inspect architecture, constraints, recent work, and dirty diff. Use graph discovery first; verify stale graph references against current source. Delegate one bounded data/monster-HP review.
2. Inspect simulation save/load, rule-pack validation and use, and externally supplied data. Prioritize concrete correctness failures over style and known parity gaps.
3. Run domain verification through local NAT tools where useful, plus current CI checks. Capture tool output outside the repository and return compact results. Validate findings with focused reproductions where possible.
4. Report actionable findings with severity, exact source locations, trigger and impact; record checks and review limits. Do not change implementation.

Token controls: one narrow reviewer, bounded graph results, targeted source reads, cached build artifacts, log files with failure summaries, no repeated successful checks.

Status: complete. Reviewed HEAD `c134541` plus working changes on `feat/monster-hp-d8`. Test edits present initially disappeared during the review through outside changes; implementation findings below were checked against the remaining tree. No implementation or golden constants were edited.

Repair follow-up: T1 subsequently resolved findings 1–5. The user explicitly authorized sampled vanilla HP and affected golden updates. Independent review and all required gates passed; 695 workspace tests include all three golden checks. Findings 6–7 are still pending. The findings below retain their original review context; see [execution plan](2026-10-06-agentic-loops.md) for current status.

## Findings

### 1. P1 — Factory migration blocks tests and golden verification (uncommitted)

`crates/nethacked-data/src/monsters.rs:192` now requires an RNG argument, but callers such as `crates/nethacked-data/src/lib.rs:44` and `crates/nethacked-agent/src/bin/demo.rs:124` still supply two arguments. Clippy, workspace tests and the golden command fail with E0061. The edited parity test also has malformed calls/unbalanced delimiters at `crates/nethacked-data/tests/ruleset_vanilla.rs:160–162`; rustfmt reports a parse error. Complete the migration and repair the parity test before assessing determinism. Existing fixed-HP parity assertions also need a deliberate RNG-aware design rather than unrelated assertion weakening.

### 2. P1 — Gameplay never calls the new HP sampler (uncommitted integration gap)

`crates/nethacked-data/src/ruleset.rs:457–463` still copies `def.base_hp` and `def.max_hp`. The by-ID factory delegates to it, and world/level generation uses those factories. Consequently the patch changes legacy callers and bones RNG consumption while ordinary gameplay retains fixed HP. The harness observes a red dragon with 5 HP through the free factory and 68 through Ruleset. Route gameplay spawning through one ruleset-aware sampler using the world's RNG, with an explicit policy for pack-provided HP.

### 3. P1 — Adult dragons receive only 4–8 HP (uncommitted)

`crates/nethacked-data/src/monsters.rs:162–164` returns `4 + rn2(5)` as total HP, independent of level. Local C `NetHack-5.0.0/src/makemon.c:1032–1036` uses `4*N + d(N,4)` before endgame and `8*N` in endgame. A level-15 red dragon should therefore have 75–120 HP before endgame; the harness gets 5. Use the full dice formula and appropriate spawn context.

### 4. P1 — Rider branch weakens Riders and ordinary wargs (uncommitted)

`crates/nethacked-data/src/monsters.rs:168–174` includes WARG and returns one d8. Wargs are ordinary level-7 monsters; actual Riders use 10d8 (local C `makemon.c:1021–1024`, plus its minimum-HP adjustment). The harness gets Death=6 and warg=2 HP. Restrict the branch to the actual Riders and roll the correct number of dice.

### 5. P2 — Golem classification tests a nonexistent flag (uncommitted)

`crates/nethacked-data/src/monsters.rs:145` checks `"golem"`, but generated M1/M2/M3 flags do not contain that classification. Golems fall into ordinary dice rolling: the harness gives a straw golem 19 HP. The original catalog's 14 HP was also only an average; C `makemon.c:2233–2263` specifies a fixed 20 HP for straw golems. Classify by actual species/class metadata and use the C fixed values. The adjacent `"elemental"` check is likewise unreachable; its proposed 5–9 total HP formula also differs from C's context-dependent home-elemental multiplier.

### 6. P2 — Stun and confusion expire twice as fast (committed)

`crates/nethacked-sim/src/turns.rs:24` calls `tick_hero_afflictions`, then line 34 calls core `tick_afflictions`. Both decrement stun/confusion (`ad_effects.rs:496–505`, `nethacked-core/src/afflictions.rs:31–37`). The harness sets both timers to 4 and waits: one elapsed turn leaves both at 2 instead of 3. Even-duration effects can also reach zero in the core tick without the sim recovery message. Give each timer one decrement owner and preserve end-of-effect events.

### 7. P2 — Pack-added random monsters are discarded (committed)

`crates/nethacked-sim/src/actions/stairs.rs:755` projects the selected definition to its optional built-in ID. Pack additions intentionally receive `id: None` in `nethacked-pack/src/merge.rs`, while `rndmonst` supports their frequency. Selecting one therefore consumes generation RNG and creates no monster. The harness selects a positive-frequency custom definition and confirms the ID projection drops it. Spawn from the selected definition/name instead of requiring a built-in ID.

## Verification

Used local NAT domain runners through `uv` and `.venv` without a remote LLM. Logs and a standalone reproduction harness are under `/tmp/netrust-review-2026-10-06/`. Cargo gate logs retain the NAT tools' bounded output tails; the harness log is complete.

| Check | Result |
| --- | --- |
| rustfmt | Failed: malformed parity test, plus formatting differences |
| Clippy, workspace tests | Failed: incomplete factory migration; clippy also rejects `iter().any()` checks |
| Golden determinism | Blocked at compilation of demo callers; no fingerprint comparison ran |
| WASM build, Python binding check, Lean build | Passed |
| Doc links, static-table guard | Passed |
| C extraction, generated Rust, behaviour coverage, parity summary | Passed |
| Standalone harness | Reproduced double timer decay, special HP errors, fixed Ruleset HP, and custom-definition ID filtering |

## Review limits and planning corrections

This is a risk-focused review, not an exhaustive file-by-file audit. Reads covered current HP changes, generation, combat/affliction integration, save/load, and pack merge/validation/load/install paths. No browser E2E or security fuzzing was performed. Save/pack reads produced no additional demonstrated defects within this scope.

The graph still references `crates/netrust-*`; current-source reads were necessary. The old global WASM constraint forbids `nethacked-pack`/`toml`, but the later web-pack design explicitly adds them and both appear in the actual dependency tree. Reconcile the constraint documentation rather than treating the intentional dependency as an unexpected regression. The NAT auditor returned no report for `wasm_deps`, so dependency evidence came from a direct locked `cargo tree`.

Delivery discrepancy: commit `7cee518` (merged in #22) describes a full `mksobj_init` implementation, but its actual diff is only seven lines: a dependency/lock update and documentation/imports. No initializer exists, and `actions/stairs.rs:764–765` still records per-kind initialization as unported. Keep that work outstanding in planning; do not count the commit message as completion.

Suggested fix order: restore compilability; centralize ruleset-aware HP sampling and correct special cases; fix timer ownership and custom monster spawning; rerun gates and actual golden comparisons. Preserve golden expectations during fixes unless an intentional behaviour change is separately authorized.

T2 follow-up: finding 6 resolved. Core countdown ownership prevents duplicate stun/confusion ticks; recovery events are emitted on expiry once. Independent review and the full gate passed; goldens unchanged. Finding 7 remains pending.

T3 follow-up: finding 7 resolved by spawning the selected definition by name. Real loaded-pack generation regression fails with the old selector and passes with the repair. Independent review and the full final gate passed (700tests and all3 goldens). All seven reported findings are now repaired; the separate `mksobj_init` delivery discrepancy remains outstanding.

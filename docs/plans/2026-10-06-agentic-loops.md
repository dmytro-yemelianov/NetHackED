# Agentic repair loops — NetHackED

Planning status: complete; independently checked with `gpt-6-luna` / `high`. This document plans execution; it does not start repairs or change live model/NAT settings.

Execution 2026-10-06: T1 started after user approval. The user explicitly selected C-correct sampled HP by default, with affected test expectations and goldens updated after verification. That decision supersedes the frozen-golden rule for this intentional HP/RNG change. Parent owns rebaselining after independent review; explicit pack HP overrides remain fixed, custom definitions default fixed, and bones ghosts keep their supplied HP without discarded RNG draws.

T1 accepted: independent Sol/high review passed; full workspace 695 tests (55 suites) and all three golden tests passed. Formatting, final Clippy, WASM, Python, Lean, static-table guard, C extraction, generated Rust, coverage, parity summary and doc links passed. Only `EXPECTED_RUNS` and `EXPECTED_COMBAT` changed; `EXPECTED_LOGS` remained identical. Logs, original golden source and exact old/new fingerprints are in `/tmp/netrust-t1-gates/`. Verification used `/tmp/netrust-t1-target` after shared-target build artifacts disappeared. Agent token usage was not exposed; no usage totals are claimed. No commits were made. T2/T3 remain pending.

Input: [repository review](2026-10-06-repo-review.md), HEAD `c134541`, and the dirty `feat/monster-hp-d8` tree. Recheck these before execution because the workspace changed during review.

## Roles and model routing

These identifiers are available in this session. They are execution choices, not performance or price guarantees. Always set both model and effort explicitly for workers; avoid accidental inheritance of an expensive parent configuration.

| Role | Model / reasoning | Assignment |
| --- | --- | --- |
| Coordinator | Parent session; planned default `gpt-6.1-sol` / `low` | Own task contracts, decisions, ledger and acceptance; delegate coding rather than rediscover it |
| Optional explorer | `gpt-6-luna` / `high` | One missing fact or bounded call-site map; skip when the review already supplies sufficient evidence |
| Implementer | `gpt-6.1-sol` / `medium` | API migration, timer fix and custom-species spawning |
| HP implementer | `gpt-6.1-sol` / `high` | Ruleset-aware HP design, C formulas, spawn context and RNG ordering |
| Independent reviewer | `gpt-6.1-sol` / `medium`; `high` for HP | Read current diff plus acceptance criteria; return concrete defects, not another broad repo audit |
| Escalation adviser | `gpt-6-astra` / `low`, then `medium` only if needed | One unresolved design/semantic question after two evidenced unsuccessful repairs; no routine use |
| Verifier | No LLM | Run local NAT domain functions and commands; record exit codes, test counts and logs |

Official OpenAI documentation supports narrow agent responsibilities, explicit model/effort selection, Luna for focused exploration and Sol for review. The exact task routing and retry limits here are project recommendations. See [Subagents](https://learn.chatgpt.com/docs/agent-configuration/subagents) and [GPT-6 guidance](https://developers.openai.com/api/docs/guides/latest-model).

Concurrency: coordinator plus at most one active child. Implementer and reviewer run sequentially; one source writer and one Cargo runner at a time. A reviewer never edits source. No nested delegation. Agent roles are logical roles, not a fleet of persistent sessions.

In this session's collaboration API, explicit worker overrides require `fork_turns="none"` or a bounded positive fork; use `"none"` and a compact task packet. Do not copy the full conversation. A model switch requires a new worker with a short handoff; `followup_task` reuses the existing worker/model.

## Preflight and task order

Preflight is coordinator-owned: record HEAD, branch, dirty-file list and a diff digest; preserve unrelated edits. Search the graph first, but verify its old `netrust-*` paths against current source. Do not switch branches underneath another worker. Before eventual commits, reconcile the obsolete P1 branch instruction with the current feature-branch policy; never commit to main.

Resolve the HP policy before implementation: how pack HP overrides interact with C-derived sampling, which spawn context is supported, and whether this task intentionally changes vanilla RNG consumption. Existing USER/P1 directives freeze goldens, while the later handover allows legitimate rebaselines. Record the authoritative decision; default to keeping expected constants intact until that conflict is resolved. This planning task requires no decision now.

The old WASM ban conflicts with the later browser-pack design. Prepare a precise constraint update reflecting the intended dependency boundary; do not remove browser-pack functionality to satisfy stale text or silently ignore the discrepancy.

| Acceptance unit | Owner | Work and exit criteria |
| --- | --- | --- |
| T1: coherent HP migration, findings 1–5 | Sol/high | Repair malformed parity calls and all affected signatures; centralize sampling over the owned Ruleset, route real spawn paths through the world RNG, preserve pack semantics, and implement correct C special cases. Review formulas, classification and RNG draw order together. Syntax-only/API repair is an internal checkpoint, not an accepted green task. |
| T2: timer ownership, finding 6 | Sol/medium | One decrement per elapsed turn for each affliction; recovery message exactly once. Cover durations 1, 2 and 4, and multiple hero actions within a game turn. |
| T3: custom monster spawning, finding 7 | Sol/medium | Spawn selected definitions without requiring a built-in ID. Validate a pack-added positive-frequency monster through real level generation, including genocide/class filtering and peacefulness. |
| T4: final integration | Coordinator + tools | Independent acceptance check, full required gates, all golden comparisons, diff/constraint audit and concise handover. No commits until the entire required gate is green. |

T1 precedes T2/T3 because it establishes the spawning API. T2/T3 are then sequential to avoid shared-tree conflicts. Each accepted task gets before/after entries in the required P1 progress ledger and a complete gate run. Do not implement `mksobj_init` as incidental cleanup: its missing delivery needs a separate specification and acceptance unit.

## The task loop

1. **Contract.** Coordinator supplies one outcome, allowed files, dependencies, invariants, exact reproduction, acceptance tests and baseline digest. Update the ledger before dispatch.
2. **Implement and reproduce.** Worker inspects only missing context, makes the smallest coherent patch and runs focused checks. Reuse the prior `/tmp/netrust-review-2026-10-06/harness` as evidence if still present; put durable regression coverage in the appropriate crate. Temporary files are not the only acceptance evidence.
3. **Independent review.** Give a fresh reviewer the contract, current diff, C references where relevant and focused-check results. Review the patch without the implementer's argument for why it is correct. Every finding needs a trigger, current location and consequence. Reviewer has no source-write authority in its task contract.
4. **Repair.** Send only confirmed findings back to the existing implementer. Recheck the affected behavior and review only the changed portions plus their integration effects.
5. **Verify and accept.** Coordinator owns execution evidence. Run the full gate once after focused checks and review pass. Accept only if criteria and gate results match the current source digest. Update ledger and send a compact result.

States: `ready → implementing → focused-pass → reviewing → verifying → accepted`. A review defect returns to `implementing`; a semantic golden mismatch returns to design diagnosis, not baseline regeneration. Distinguish `compile-blocked`, `assertion-failed`, `environment-blocked` and `passed`. A stale source digest invalidates acceptance evidence.

At most two repair cycles on the same unresolved defect with the same model. After that, send one focused escalation packet: failed hypothesis, minimal evidence and the precise decision needed. Use a stronger effort/model only for that question, then return to the normal worker. Do not restart broad exploration or repeat the same failing command without a changed hypothesis. Infrastructure failures get one justified retry. If the remaining dependency needs user input, preserve evidence and report it instead of cycling agents.

Across all models, allow at most three repair cycles per underlying defect, one escalation event and at most two adviser turns (Astra/low, optionally Astra/medium). The returned worker gets only the final remaining repair cycle. Rephrasing a finding, switching models or starting a new session does not reset these counts. If that attempt fails, stop dispatching repair agents for the defect, record `needs-decision` with the smallest unresolved question, and let the coordinator revise the task contract or obtain the genuinely missing information. Do not report acceptance.

## Verification loop

Use `uv` with `.venv` and invoke NAT domain functions directly; running deterministic tools does not need a remote ReAct agent. Current NAT configs still name Llama 3.2 Vision or GPT-4o; those configs are not the implementation/review routing above. Do not insert session-specific model names into an API-backed NAT config without checking backend availability and its schema.

For each patch: format check, focused compile/tests, independent review, then the required gate:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --exclude nethacked-py --locked --no-fail-fast
cargo test -p nethacked-agent --test golden_determinism --locked
cargo build -p nethacked-wasm --target wasm32-unknown-unknown --locked
cargo check -p nethacked-py --locked
bash scripts/check-no-static-tables.sh
uv run --no-sync python scripts/extract/c_tables.py --check
uv run --no-sync python scripts/extract/gen_rust.py --check
uv run --no-sync python scripts/behaviour-coverage.py --check
uv run --no-sync python scripts/parity-summary.py --check
bash scripts/check-doc-links.sh
lake build
```

Treat the list as required coverage. If the workspace test log already proves that every golden test ran and passed, record that evidence rather than running the standalone golden command again. Verify `EXPECTED_RUNS`, `EXPECTED_LOGS` and bestiary combat; never interpret a compilation error as a fingerprint mismatch. If the first Rust compile fails, diagnose before launching more equivalent Rust gates; finish outstanding gate coverage after repair. Independent docs/data checks may still run.

Capture complete command output outside the checkout where possible. NAT's bounded tail is a summary, not a complete log. Report exit code and failing test/compiler location; on success report test counts and an evidence path. After a full gate passes, repeat it only for a new source change or a demonstrated concern. The final task may reuse a complete gate against the identical tree and lockfile. Check tools' real exit status; a blank auditor response never means clean.

## Token controls and handoffs

The following are soft envelopes, not runtime-enforced quotas or cost forecasts. Count input, output and reasoning where usage is exposed; record unknown usage rather than inventing totals.

- Initial task packet: target 1,200 tokens or less; cite paths and symbols instead of pasting files.
- Source retrieval: bounded graph results and 80–160-line snippets. No generated-table dumps or full test logs in model context.
- Worker/reviewer handoff: target 300 tokens; at most 3 evidenced findings per review pass. Retain additional findings in an artifact so they are not lost.
- Model work per acceptance unit: aim for 8k tokens across worker/reviewer; allow 16k for the HP unit. Reassess at the envelope before adding another agent. Reduce context or split a coherent independent task; do not skip required correctness work.
- Skip an explorer when the task packet already gives the failing path. Reuse a worker for a narrow repair, but use a fresh reviewer for independent acceptance. Pass incremental diffs, not repeated whole histories.
- Keep coordinator reasoning low for routing/status; use Sol/high for HP semantics and Astra only after evidenced escalation. Never turn on maximum effort by default.

Task packet template:

```text
TASK / baseline HEAD + diff digest / allowed files
Outcome + reproduction
Constraints + resolved behavior/RNG policy
Acceptance tests + relevant C/source references
Tool authority: one writer; no commits; no nested agents
Return: status, changed paths, checks, remaining risks; <=300 tokens
```

Completion record: task ID, baseline/final digests, model/effort, repair count, usage if available, changed paths, review findings resolved, gate/test outcomes and log paths. Completion follows tested source, not a commit message or agent claim. Deliver a patch or reviewable commit only after the required checks pass and existing commit policy is satisfied; publication is outside this planning task.

Planning validation: documentation-link check passed. Independent review's retry-cap finding was incorporated. Its suggestion to defer all full gates was declined because the current global constraints require each accepted task to remain green; final integration reuses same-tree gate evidence to avoid redundant runs. Existing trailing whitespace in the dirty monster source was left untouched. No Rust tests were rerun for this documentation-only plan.

## T2 execution — 2026-10-06

T1 committed as `049ed98` after explicit user authorization. T2 accepted after independent Sol/medium review and the full required gate: 699 workspace tests, including all three unchanged goldens. Core owns all five transient countdowns; simulation recovery messages and blindness clearing derive from transitions. Four regressions cover durations 1/2/4, fast hero actions, recovery once, and lethal countdown precedence. Fatal ticks preserve core early returns and leave transient counters unchanged, without recovery events. No escalation or repair cycle was needed; token usage unavailable. Evidence: `/tmp/netrust-t2-focused.log` and `/tmp/netrust-t2-gates/`. T3 remains pending.

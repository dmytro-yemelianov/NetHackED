# Parity process optimization

Effective after SAVE1 commit `a6c12aa`, on the user's instruction to finish that unit and write the process before further implementation. This amendment takes precedence over conflicting efficiency/model defaults in the [agentic loops](2026-10-07-full-parity-agentic-loops.md) and [autonomous contract](2026-10-07-autonomous-parity-run.md). It changes execution, not the [53-family scope](2026-10-07-remaining-parity-scope.md), independent acceptance, golden permissions or repair limits.

## Measured problem

### 2026-10-09 correction after session-composition review

The user requested reconsideration of the approach. The B review used 214,452 uncached input / 37,842 output tokens in 1,285 seconds; its repair used 169,361 / 46,340 in 1,582 seconds. Repeated declarations of narrower context have not controlled cost. These rules supersede conflicting execution defaults below; acceptance requirements, scope, frozen tests and per-cause retry history remain intact.

- A coordinator may implement a small, concretely reproduced repair directly when no child writer is active. A separate implementation agent is optional. The repair still receives independent review; the coordinator cannot accept its own code.
- Give a reviewer the candidate diff, reproduced failures and affected requirements. Preserve acceptance of unchanged behavior. Follow-up review covers the repair and its actual effects; a broader review needs a concrete new finding. Resolve findings from the existing coverage map instead of producing another full map or implementation history.
- One Cargo runner means one concurrent process, not one invocation. Correct a failed fixture and rerun its target normally. Do not introduce direct-rustc compilation or package reconstruction merely to keep an artificial invocation count at one.
- Root owns mechanical evidence. Reuse the existing runner and input manifest; keep one baseline and final source binding per candidate, the diff, command results and raw failure logs. Reviewers inspect those bindings and write findings. Do not ask models to create another evidence framework or reconstruct every intermediate source version after the work is finished. Record missing historical provenance explicitly; it cannot support an acceptance claim.
- During repair, run the reproducing test and affected regressions. Run required broader checks on the stable candidate, and the full gates/goldens before commit. Repeat only checks invalidated by changed inputs or a concrete unresolved concern.
- A packet states the finite acceptance checks and stopping condition. Return a concise verdict or a concrete incomplete checkpoint when those checks cannot finish; do not keep expanding documentation or predecessor audits after the result is known. Report measured usage without claiming savings or a live token cap.

The already-running readback finishes under its bound packet. Apply this correction to subsequent work; no concurrent source edits. The next action is to repair only its confirmed findings, obtain independent delta acceptance, then run the required final gates and commit. This amendment requires no additional process-review agent or separate optimization program.

SAVE1 implementation took 28.25 minutes and its independent review 20.30 minutes. Its 12 final gates took 191.29 seconds. The implementation reported 102,597 uncached input tokens (total input minus cached input), 29,462 output tokens; review reported 117,608 and 19,794 respectively. These are CLI telemetry, not weekly allowance percentages or API billing. Reasoning is recorded separately and is not added to output without confirmed telemetry semantics.

The final 11-command focused allocation was repeated three times after fixture refinements. Failed save assertions exposed complete serialized byte arrays through very long log lines. Reports and mechanical source/archive proofs were duplicated. The first optimization therefore removes repeated exposure and repeated work; it does not remove verification. The external `PROCESS-OPTIMIZATION/metrics.json` under the run's evidence root preserves the measurements and source hashes.

## 1. Scope and dispatch

Use the existing run ledger as the queue; never load the complete registry into a worker. One unit covers one independently testable behavior with explicit prerequisites, changed-file allowlist, pinned C branches, public observable outcomes and exclusions. Reuse an accepted contract through its hash. A separate contract-writing agent is needed only for unresolved semantics or dependencies, not every small change.

Split before dispatch when a unit spans multiple lifecycle owners, requires unrelated compatibility migrations, or cannot fit the packet below. Do not fragment one invariant merely to create more commits. Existing ready contracts remain usable. Newly discovered scope gets a child ID; child acceptance still does not close a family.

Only one active child plus coordinator, one writer and one Cargo runner; no nested agents. The coordinator selects work, binds evidence and commits. It does not port the same code or repeat a semantic review while waiting.

## 2. Model routing

| Work | Starting model / effort | Escalation trigger |
|---|---|---|
| Mechanical checks, hashing, metering, log summaries | Local tools, no model | A concrete interpretation question |
| Narrow facts, inventory or documentation | `gpt-6-luna/medium` | Missing or conflicting evidence, not elapsed time |
| Independent planning-only process check | `gpt-6-luna/high` | A concrete unresolved policy defect |
| Normal implementation | `gpt-6.1-sol/medium` | Demonstrated timing, ownership or ordering complexity |
| Difficult lifecycle/RNG implementation | `gpt-6.1-sol/high` | Existing bounded repair policy |
| Fresh structural/documentation review | `gpt-6.1-sol/medium` | A semantic ambiguity |
| Fresh semantic code review | `gpt-6.1-sol/high` | Existing bounded repair policy |

Skip a facts agent when paths and contracts are known. Use a fresh session with an explicit packet (`fork_turns="none"`), not inherited history. Preserve the existing limit of two unsuccessful same-model repairs and three total per defect. After the second, permit one narrow Astra/low advice event, with an optional second turn at medium and at most two adviser turns in that event. Model/effort changes, resumes and reworded findings never reset counters. Infrastructure gets one changed-hypothesis retry. Availability failures do not permit self-review or silent model substitution.

These are project starting settings, to be evaluated on accepted work. Official [model selection](https://developers.openai.com/api/docs/guides/model-selection) recommends matching capability to the task; [reasoning guidance](https://developers.openai.com/api/docs/guides/reasoning) describes the effort/latency tradeoff. No price or savings claim follows from this table. The parent session's model cannot be changed by a worker routing instruction.

## 3. Context and output

Before launch, count the task packet using a recorded tokenizer/encoding: at most 1,000 estimated tokens. An estimate is not backend usage. Packet fields: unit/HEAD, applicable AGENTS/USER/global constraints and recorded exceptions, exact contract and evidence paths, allowlist, acceptance conditions, development test target, final required checks, exclusions, retry counters and compact return schema. Do not embed full logs, transcripts or catalog rows.

Discovery follows AGENTS.md: graph first, then known-path fallback when insufficient. Start with 5–10 graph results or 80–160 source lines. Every model-facing tool response has a 2,000-token ceiling; larger relevant source is read in targeted chunks. Preflight log bytes and longest-line length. Parse raw failures locally and return test name, error, first differing offset/field and counts. Never dump full save byte arrays. Preserve raw logs externally so a reviewer can independently inspect or summarize them. Truncation must not be presented as complete evidence.

Handoffs are at most 250 words; findings at most 300 words, with structured fixture IDs and durable evidence paths/hashes outside that prose. Passing review needs a verdict, checked obligations, bindings and residual risks, not a second implementation history. Root owns mechanical complete-input/archive/copy verification; reviewer independently checks semantic source and relevant evidence bindings. The same source proof is referenced across phases rather than narrated again.

## 4. Validation and evidence reuse

During edits run only the changed behavior's test target. Once source and fixtures stabilize, run the contract's final focused allocation once. A later change reruns affected checks; unchanged results are reusable only when every relevant input, command, tool/configuration and fixture binding is identical and the reason is recorded. A new failure or finding invalidates the relevant reuse. This does not waive contract-specific compatibility checks.

After fresh independent semantic PASS, the coordinator runs all 12 gates and all three unchanged goldens on final production inputs before a code commit. Workspace output may supply the three golden results; avoid a duplicate standalone golden run. If source changes after review/gates, invalidate the affected acceptance evidence. Pure documentation checkpoints may reuse the complete same-source gate set after verifying the entire input set and bytes, with fresh documentation/parity checks. Final full-goal integration gates remain mandatory.

Store full logs/snapshots externally with durable paths and hashes. Publish a compact unit record and summary, plus required truthful spec/parity updates. Update P1 progress before and after a task. During an unchanged wait update the handle state, not an append-only narrative every poll. Commit only explicit paths with required trailers and verified parent/branch/index/blob bindings.

## 5. Metering and checkpoints

Replace the old ambiguous combined input/output/reasoning envelopes with these per-unit soft reassessment thresholds across coordinator and children where exposed:

| Unit | Uncached input | Output |
|---|---:|---:|
| Facts/docs | 12,000 | 4,000 |
| Normal behavior | 30,000 | 8,000 |
| Difficult lifecycle | 50,000 | 12,000 |

These initial thresholds are control triggers, not a user-approved spending budget or measured optimum. Record total input, cached input, their difference, output, reasoning, wall time, model/effort, retries, repeated checks and evidence paths. Never sum cumulative usage snapshots twice or silently omit a child. Missing or inaccessible coordinator/live usage is `unknown`; do not claim a unit stayed under threshold. At each terminal phase, exceeding a threshold or missing telemetry requires a recorded decision to narrow context/split coherent scope before another model call. Never discard required checks to meet a threshold.

Current CLI usage is terminal telemetry, so these thresholds cannot interrupt a live call or guarantee a hard quota. Monitor live usage only when actually exposed. The [local execution guard](../../scripts/parity-agent-guard.py) now checks packets before launch, persists captured inputs, and reports terminal usage. The coordinator still enforces the phase checklist and decides adjustments; no live quota controller has been deployed. Preserve checkpoints when runtime limits stop execution. Poll the same verified handle; timeout is not termination. Wait mechanically, without future-source audits or duplicate workers. User updates state what changed or remains pending, briefly.

## Adoption and launch checklist

1. Obtain one fresh Luna/high read-only process review; resolve concrete findings. Verify this doc-only change against SAVE1's complete 346-input gate binding and run fresh doc/parity checks. Commit the process before another parity writer.
2. Before every launch record: coherent unit, dependencies, model/effort rationale, packet token estimate, allowlist, relevant input bindings, focused/final checks, output ceilings, retry state and prior-phase usage decision. A missing item prevents launch.
3. At terminal phases record actual telemetry, changed paths, compact handoff/verdict and affected-check/reuse decision. Exceeded/unknown usage needs the explicit adjustment above before the next call.
4. Pilot on the next dependency-ready unit. Compare phase telemetry, repeated test allocations, wall time and review defects with the retained baseline. After three accepted units, tune thresholds/routing once from evidence; do not start a separate optimization agent for every unit.
5. Later automation work, if needed, is a bounded task implementing packet/output/telemetry guards. It requires its own review and appropriate gates. No new parity dispatch occurs before step 1; the full parity goal remains active throughout.

## Local guard adoption

Use `uv run --no-sync .venv/bin/python scripts/parity-agent-guard.py` with `preflight PACKET`, `launch PACKET`, `usage RUN_DIRECTORY` or `excerpt FILE`. The packet includes explicit prior-phase reports and adjustments; prompt/schema and evidence files are bound by SHA256. The launch takes a fresh external run directory and rejects replay. Output schemas and bounded excerpts reduce exposure; the helper does not intercept an agent's own tools.

The current launch supports a workspace-write runner with repository cwd. A review launched through it must have explicit no-source-write instructions; complete source hashes are checked afterward. Writable temporary test files are permitted. Independent review remains a separate fresh session. Global writer/Cargo ownership and complete declaration of all unit phases remain coordinator responsibilities.

The guard's own final repair and reviews exercised actual preflight/launch/report commands. Twenty local tests and 155 independent malformed-input probes passed after two bounded repairs to the same snapshot-validation defect (one unsuccessful repair retained). Raw evidence lives under `PROCESS-GUARDS` in the run evidence root; the ledger binds its review, usage and final gates. All observed guard phases exceeded their soft input threshold; this is not evidence of savings. Before the parity pilot, use a narrow accepted contract and record a new cumulative-usage adjustment.

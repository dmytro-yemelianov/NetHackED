# Autonomous completion contract for remaining parity

Prepared at `5edce53`, 2026-10-07. This is a launch contract and initial ledger, not an activated parity run or an executable scheduler. Codex Goals supply thread continuation; the coordinator operates the queue using the existing tools. [Official Goals guidance](https://developers.openai.com/cookbook/examples/codex/using_goals_in_codex) describes persistent objectives with verifiable outcomes, constraints and stopping conditions.

Authority remains [AGENTS.md](../../AGENTS.md), [USER.md](../../USER.md), [MEMORY.md](../../MEMORY.md), the [scoped parity plan](2026-10-07-remaining-parity-scope.md) and the [agentic execution rules](2026-10-07-full-parity-agentic-loops.md). The [scope registry](../parity/obligations/2026-10-07-remaining-parity-scope.json) defines 53 families across seven streams and all 543 historic rows. The [run ledger](../parity/obligations/2026-10-07-autonomous-parity-run.json) initially records an unstarted run; coverage instances are not implementation tasks. An active thread goal must name this outcome. The previously observed goal text `remove` does not identify the parity objective; clear or replace it through the user's goal controls before launch.

Branch authority is the existing user-authorized repair branch `feat/monster-hp-d8`, recorded in MEMORY.md under “Original P1 branch … current authorized repair branch”. That recorded user exception overrides the original `feat/rule-packs-p1` branch line in the P1 global constraints; it does not override other constraints or authorize commits to `main`. Revalidate the branch and any newer user directives before each checkpoint. No new branch authorization is introduced here.

## Goal to activate

```text
Complete all seven remaining NetHack parity streams defined by
docs/plans/2026-10-07-remaining-parity-scope.md and its obligation registry,
following docs/plans/2026-10-07-autonomous-parity-run.md.

Audit all 53 families and all 543 historical rows against released NetHack
5.0 source commit 16ff59115315917b93185d026aeefea06db9b0f4 under a reviewed,
pinned configuration. Enumerate every required catalog and branch/context;
register newly discovered obligations without shrinking the scope.

Start with the configuration bootstrap and P11a source contract. Reuse
accepted evidence, split coherent children, select accepted prerequisites,
implement or regression-verify, obtain fresh independent review, run full
gates and all three goldens, update the ledger/docs and commit each accepted
checkpoint. Continue automatically to the next dependency-ready unit.

Use the existing token-efficient model routing, one active child, one writer,
one Cargo runner, repair limits and compact handoffs. Preserve owned data,
determinism, compatibility and all existing assertions. C remains source-only.
Keep future goldens frozen unless an exact reviewed change receives explicit
user authorization. No push, merge, deployment or expanded optional scope.

Finish only when the completion audit in this contract passes for every
family, row, member and context, with current full gates and no unresolved
in-scope gaps. If one path is blocked, continue safe independent work.
If no safe progress remains, report the concrete blocker and required input;
mark blocked only at the runtime's repeated-blocker threshold. Never call a
blocked or partly completed run successful. Report actual token usage when
available; do not invent estimates or savings.
```

Use `/goal` with this objective in a Goals-enabled thread. The contract supplies the outcome; merely saving these files does not start continuation. Budget limits and runtime availability can stop a run before completion. No execution budget or automatic future rebaseline permission has been supplied for this proposed run.

## Durable state and unit contracts

Keep the scope registry as the immutable planning baseline. The run ledger holds family states, discovered child IDs, inventory/catalog coverage references, decisions, the current unit, process handles, review findings, repair counters and the next action. Each child has a small versioned contract/evidence record under `docs/parity/obligations/`; store full tool logs and byte snapshots externally, with durable locations and hashes. Keep evidence needed for future verification retrievable after `/tmp` cleanup. Update the P1 progress ledger before and after each task.

A child contract must include:

- Stable ID, parent family and selected source branch/member/context; pinned C symbol, file hash and configuration conditions.
- Audited current Rust owner and public action; allowed files and exact accepted narrow prerequisite IDs, evidence hashes and consumed interfaces.
- Independent expected fixtures, draw bounds/order, state/events/turn costs, failure/boundary cases and applicable frontend routes.
- Mid-operation save/native continuation, old-save/old-pack loads and applicable schema/generated binding/canonical hash checks.
- Required validation commands, exclusion evidence and explicit successors for every deferred in-scope branch.
- Review evidence, per-defect repair counters, full-gate input binding, adjusted existing test names and accepted commit.

Family edges identify audit/shared-foundation work. They do not demand complete parent-family closure before a member can start. Register concrete prerequisite children and validate their graph before dispatch. A verified existing implementation receives independent regression evidence rather than another port.

The ledger's counts are diagnostics, never completion proof. `accepted` child states require a retrievable reviewed evidence record and accepted commit; `closed` family states require the aggregate audit below. Newly discovered children increase the queue instead of disappearing from the denominator.

## Controller loop

At each continuation, read only the small run ledger, current contract and missing evidence. Revalidate current HEAD, branch, dirty paths, input hashes, accepted checkpoints and actual live processes before choosing work. Preserve unrelated user edits. Resume packets are bounded; avoid replaying transcripts or loading the complete catalog registry into every worker.

Choose the next action in this order:

1. Poll a recorded process/agent handle if authoritative state confirms it is live. A timeout is not termination. Do not duplicate Cargo or restart a live worker. A missing/terminal handle triggers evidence inspection and recovery from actual files/logs.
2. Resume the current unit's next valid step or narrowly repair a confirmed finding within its per-defect limits. A stale input hash invalidates acceptance for affected evidence; obtain new verification instead of reusing it.
3. If a decision blocks only that path, record the exact proposal and continue independent ready work. Unaccepted source edits must be kept isolated; read-only independent audits can proceed without committing a failing tree.
4. Select a dependency-ready reviewed child. Use existing wave order, then scope-registry family order and stable child ID as tie-breakers. Prerequisite readiness takes precedence over broad waves. Initial priority is configuration bootstrap and P11a.
5. If no concrete child is ready, select the next unresolved family/catalog/inventory audit that creates a needed contract or prerequisite. Reconcile at most 20 inventory/member entries per discovery packet. Declared AT/AD/flags/properties and complete level enumeration are mandatory audits.
6. If every aggregate appears closed, perform the full completion audit. Otherwise identify why no safe action remains and apply the blocked rule; an empty ready queue is not completion.

Before the first source-conformance acceptance, pin a global configuration record: released source identity; enabled/disabled gameplay features and conditional branches; supported options and deterministic calendar/environment inputs; intended frontend applicability. Source-audit routine choices using current evidence. A material scope reduction requires an explicit decision, not an invented configuration exclusion. Record still-unknown settings; they cannot support accepted conformance.

The child lifecycle is:

```text
audit_needed -> contract_review -> ready -> implementing
-> focused_pass -> reviewing -> verifying -> accepted
```

`needs_repair`, `needs_decision` and `environment_blocked` preserve the parent, failed evidence and retry counters; they are not acceptance states. Audit-only children skip implementation through a reviewed existing-path finding and appropriate verification. Infrastructure repair, input changes or new findings justify reruns; entering another phase alone does not.

Each accepted checkpoint gets truthful obligation/parity/spec updates, the full gate checks and golden results bound to the final reviewed inputs, then a coordinator commit with required trailers on authorized `feat/monster-hp-d8`. Pure documentation units may reuse complete same-source verification under the existing input-equality rule, with fresh changed documentation checks. Final goal acceptance requires fresh full integration gates on the final production inputs. No fixed test count is a success criterion.

## Models, token use and stopping rules

Use the existing session identifiers: normal worker `gpt-6.1-sol/medium`; difficult timing/lifecycle/ordering worker `gpt-6.1-sol/high`; fresh semantic reviewer `gpt-6.1-sol/high`; optional narrow facts `gpt-6-luna/high`. One active child plus coordinator, one source writer and one Cargo runner; no nested fleet. Use `fork_turns="none"` with explicit packets. Workers never commit, edit goldens or approve their own evidence.

Retain the existing soft work envelopes (4–6k structural, 8–12k normal, ≤16k difficult), ≤1,000-token task packets and compact handoffs. Split at a natural semantic boundary when a unit grows; preserve checks and scope. Usage is `unknown` when tools do not expose it.

For a single underlying defect, keep at most two unsuccessful same-model repairs and three total repairs. After the second, permit one narrow `gpt-6-astra/low` advice event, optional second turn at medium, maximum two adviser turns. Do not reset counters on a model switch, resume or reworded finding. Infrastructure has one justified changed-hypothesis retry. On exhausted repairs, preserve a concrete decision packet and pursue independent work where possible.

Completion, user pause, runtime budget/availability, and a genuine no-safe-progress blocker are distinct outcomes. A runtime goal can be marked blocked only after the same impediment repeats for three consecutive goal turns and no meaningful progress remains. Mark paused only on explicit user instruction. Never stop merely because the current child finished while the goal remains incomplete.

The current golden policy can require user input: only previous HP/item/P04–P10 decisions are authorized within their recorded scopes. A new intentional mismatch needs an exact affected-value proposal, independent semantic review and recorded user decision before coordinator updates. The run contract does not change that policy or weaken test logic. A fully unattended run through such mismatches needs a separately approved standing policy; these files grant none.

## Final completion audit

Completion requires current, retrievable evidence for every condition:

1. All 53 families have reviewed source/configuration branch inventories, including discoveries absent from historical tables. Every scoped branch and context is accepted or demonstrably outside the reviewed configuration; no silent unsupported slice or material scope reduction.
2. All 543 historical rows, including the 59 historically present rows, are reconciled to current source and public regression evidence. All currently in-scope missing/partial/uncertain behaviors are closed. Any nonapplicability has reviewed configuration evidence; historical status counts alone prove nothing.
3. Every extracted object/species/artifact/role/race/command member and applicable context is covered. Declared attack/damage tags, flags, properties and full level variants/topology have complete source enumerations; reused 124-level/85-command/USED-tag counts are not proof of completeness.
4. Intended frontend actions, selection/prompt/cancel/repeat/turn behavior and public content placement are tested. Rust saves/native RNG continuation and applicable old-load/schema/hash guarantees pass. Helper-only tests or names in code cannot close a public feature.
5. All discovered child contracts, reviewed findings and explicit successors resolve to accepted evidence. There are no open in-scope gaps, unknown configuration settings, exhausted unreconciled repairs, pending required decisions or untracked implementation changes.
6. A fresh independent integration audit reviews aggregate coverage and representative cross-system interactions on final inputs; it returns no required findings. It may reuse accepted semantic evidence but must verify bindings and investigate weak/missing coverage.
7. All 12 required gates and all three goldens pass on the final reviewed production inputs. Record HEAD, complete sorted input hashes/digest, commands, logs, exits and actual test results. Do not infer C conformance from Rust determinism alone.
8. Final obligation tables/spec/docs remain truthful, accepted changes are committed, unrelated edits are preserved and the delivery record binds the accepted evidence to the resulting commits. Every explicit requirement is matched to evidence before the runtime goal is marked complete.

Publish a compact checkpoint after each accepted unit: family progress, current concrete children accepted/open, remaining mandatory audits, decisions/blockers and next action. Report discoveries separately from closures so changing task counts remain understandable. Final delivery must distinguish achieved gameplay/content/command parity from optional exact C replay, original presentation and binary C interchange.

# Ruleset-owned room item generation

Baseline: `f7b752a`, branch `feat/monster-hp-d8`, clean tree. The user authorized continuing the proposed sequence; this first acceptance unit closes ordinary-room selection ownership. Follow [agentic loops](2026-10-06-agentic-loops.md): one Sol/medium writer, fresh Sol/high read-only review, coordinator-owned local full gates and commit. No nested agents, new dependencies, golden edits by workers or weakened assertions.

## Outcome and scope

Ordinary-room item selection uses owned Ruleset item definitions and generation weights, including custom definitions with no built-in ID. Pass the selected definition/name into existing random tree initialization; do not discard custom items via an ID-only factory. Keep existing initialization, artifact-context, failed-placement and continuation semantics. Container selection already uses owned facts; preserve that behavior.

For unmodified vanilla definitions, preserve the current class draw, canonical item order, kind draw bounds and exact seeded results. Built-in weighted order is canonical C ID order; custom definitions follow a stable documented order. Artifacts and zero-weight items stay out of ordinary pools. Class proportions remain existing extracted facts; only within-class candidates/weights become owned. Define empty-pool behavior without an accidental RNG draw, panic or infinite retry. Handle weight sum overflow safely and consistently with current pack validation. Renamed overrides retain selected owned names/profiles/stats. Avoid a new handwritten fact table or translated-name classification.

Compatibility: old compiled packs that omit generation weights must retain historical selection behavior where practical. Determine omitted-versus-explicit-zero semantics from the existing format; report the precise design before editing if it requires a schema/default change. Explicit zero in newly authored packs must disable a kind. New serialized fields, if necessary, need defaults plus old-pack/save and canonical hash coverage. Preserve explicit RNG-free factories.

## Acceptance

- Deterministic scripted draw bounds and boundary selections independently checked for vanilla, custom and overridden weights; canonical/stable ordering despite reordered lookup indexes.
- Empty and all-zero pools, artifact exclusion, large/overflowing weights, absent/explicit-zero legacy semantics and owned renames covered without vacuous statistical assertions.
- Actual authored/built/loaded pack reaches public level generation with a positive-weight custom item, retains its fields/profile and excludes zero-weight items. Pin a reproducible seed or deterministic fixture. Disabling production owned selection must fail this regression.
- Public save/reload and subsequent level generation match every parsed JSON value and RNG continuation; prior item/artifact/placement assertions remain intact.
- Fresh independent review passes. Full fmt/Clippy/workspace/WASM/Python/static/extract/generated/coverage/parity/docs/Lean gates and all three goldens pass before commit. Vanilla goldens are expected unchanged; diagnose any mismatch before considering whether the prior scoped item/RNG exception applies.

Evidence goes under `/tmp/netrust-owned-items-*`, using Cargo target `/tmp/netrust-t1-target`. Coordinator updates the required P1 ledger before/after tasks and records checked source digest, review/repair count and test results. Model token usage is unavailable unless tools expose it.

Later proposed units remain separate: corpse rot/revival timers with ice-box suspension, fuel/species payloads, then artifact powers/gift contexts. Each needs its own bounded contract before implementation.

## Compatibility design checkpoint

Use per-item serialization provenance for an omitted legacy generation weight, with an optional weight in the deserialization wire representation. New/direct definitions default to explicit weights, including zero; omitted legacy fields retain omission when reserialized. The provenance flag itself is not serialized. Legacy canonical-ID fallback is a Ruleset-owned compatibility path, while absent custom weights remain ineligible. Container behavior stays unchanged. Prior old-hash fixtures may add the missing-field flag as setup only, keeping assertions intact. New vanilla canonical JSON may emit explicit zero fields; verify format/hash implications separately from unchanged seeded gameplay.

## G1 acceptance — 2026-10-06

Sol/medium implemented owned class-kind selection with canonical built-in/stable custom order, checked weight sums, artifact/zero exclusion and definition-name routing into existing initialization. Shared container weighted selection retains its raw-weight behavior. Missing legacy weights use owned compatibility probabilities; explicit zero is authoritative and reserializes as zero. Legacy omitted JSON/hash is preserved, while newly serialized vanilla definitions emit zero weights explicitly.

348 focused tests passed. The pinned seed6 authored/build/loaded-pack regression observes a custom item through public floor generation, verifies fields/profile/zero exclusion and full parsed JSON/RNG continuation over floors4/5. Restoring the old production selector makes it fail, and the owned selector passes after restoration. The old-hash fixture changes setup only to mark missing fields; assertions stay intact. Independent Sol/high review PASS with no findings or repair cycles; routine fixture/compile corrections occurred before handoff. No escalation; model usage unavailable.

Full accepted gate:770workspace tests, all3unchanged goldens and all12checks pass. Source digest unchanged through verification; Cargo.lock and complete golden source unchanged. Evidence: `/tmp/netrust-owned-items-focused.log`, `/tmp/netrust-owned-items-negative.log`, `/tmp/netrust-owned-items-gates/accepted.json`. Final integration reuses this complete same-source gate; coordinator-only documentation updates follow. This accepts G1 only; later corpse/fuel/artifact-context units remain separate. No push/PR.

# P04a: hero attribute contribution to melee to-hit

Baseline: `b912faf`, branch `feat/monster-hp-d8`, 2026-10-07. Dependencies: P01a current attribute inventory, P02 source-derived fixture convention and P03 owned actor identity. Follow the [agentic loop](2026-10-07-full-parity-agentic-loops.md): one Sol/medium writer, fresh Sol/high semantic review, coordinator-owned full gates and commit.

## Bounded outcome

Audit and connect the stored hero attributes to one ordinary hero melee to-hit formula against vendored NetHack 5.0 `weapon.c:abon` and its attack call sites. Confirm representable attribute ranges, level terms already present, equipment/form modifiers and public action paths before writing. Missing prerequisites narrow this unit or become a preceding contract. Damage bonuses, burden, exercise, progression and unrelated combat branches stay separate.

Use independent source-derived boundary expectations and a meaningful public attack regression that detects a bypassed contribution. Owned actor/definition resolution, RNG draw order, explicit pack overrides and save/RNG continuation remain intact. No new dependencies, handwritten runtime fact tables, C execution or weakened assertions. New serialized fields require compatible defaults and load tests if genuinely necessary.

## Acceptance and policy

Workers cannot edit checked goldens, commit or expand scope. Full 12 gates and three goldens are required before commit, with results bound to unchanged source/configuration input hashes. Prior HP/item rebaseline exceptions do not automatically cover attribute integration: a mismatch must first be diagnosed with concrete reviewed behavior and original constants, then any required policy decision is presented to the user. No permission question is needed before producing that reviewable result.

Record source/configuration assumptions, supported boundaries/public behavior, existing-test adjustments, review findings/repair counts, verified gates and remaining partial obligations. Closing this formula does not close broad attribute or combat parity.

## Bounded design

`HeroProgress` stores i32 base attributes, including C's encoded exceptional strength. Ordinary-form contribution follows `weapon.c:abon` strength/dexterity thresholds and its level-below-three bonus; preserve the separate existing hero-level term once. Wire the contribution into the simulation combat path reached by explicit melee actions and movement bumps. The current attribute accessor reads base values; equipment and temporary effective-attribute modifiers remain incomplete. Polymorph uses a distinct C formula whose prerequisites are absent, so preserve its existing partial path without adding the ordinary-form contribution.

Tests must cover threshold boundaries, encoded strength and low-level transitions with independent literals, plus public routing, save/RNG continuation and a meaningful bypass control. Existing hit/miss-dependent damage draws remain conditional; adding the formula itself consumes no RNG.

## Reviewed implementation

Sol/medium added the ordinary contribution with independent C thresholds and bounds; fresh Sol/high review passed with no findings. Three new tests cover 34 source-derived boundary cases and 144 public attack/bump scenarios. Removing the production contribution fails the public regression; restoration passes. All 516 core/simulation tests passed before review, with no existing test changes. The [obligation register](../parity/obligations/2026-10-07-p04a.json) keeps damage and effective-attribute/polymorph work open. Full verification against original golden constants is in progress; no rebaseline is authorized by this record.

## Concrete policy checkpoint

The original-constant full run passed 781 tests and failed only the two golden suites: three of four RUNS entries and all three COMBAT entries changed. LOGS passed unchanged. All 11 other gates passed. Fresh narrow review confirmed pristine original constants, exact proposal arrays and ordering, and consistency with the reviewed ordinary hit-chance change and conditional damage/RNG continuation. The production/test hashes and four pinned released-C source hashes remained unchanged. No constant has been edited.

Evidence: `/tmp/netrust-p04a-review.md`, `/tmp/netrust-p04a-golden-proposal.json`, `/tmp/netrust-p04a-gates/results.json`. A scoped attribute rebaseline decision is pending under P1's frozen-golden rule; the earlier HP/item exceptions do not apply automatically. This unit is reviewed and otherwise verified, but is not accepted or committed until the decision and final passing goldens.

# P04c: owned inventory capacity and public burden consequences

Baseline: `b912faf` with reviewed P04a/P04b preserved. Active goal is completion of P04–P10. One Sol/medium implementer and fresh Sol/high review, coordinator-owned policy/gates/docs/commit.

## Required outcome

Connect stored strength/constitution and owned carried item weights to C-derived capacity/encumbrance classification and actual public inventory/movement consequences. Audit existing core capacity helpers, nested containers/bag weights, hero form/equipment adjustments and movement/turn scheduling before edits. Use existing state/facts; do not invent parallel handwritten item tables or double-count contained weights.

Pin the selected ordinary-form C capacity and encumbrance branches and identify scheduling dependencies explicitly. If faithful slowdown needs P09 energy ownership, implement the independent capacity/classification/overload consequences now and keep the slowdown dependency open until P09; do not silently claim burden complete. No hidden scope reduction. Public threshold/weight/pickup/drop/movement and save/RNG tests must prove production wiring, not helper-only math. Preserve P04a/b and unrelated worktree edits.

No new dependencies/C execution/unchecked golden edits/commits/coordinator docs by worker. Existing assertions remain unchanged except setup. New serialized state, if needed, requires defaults and old-load tests. Full acceptance needs independent review, complete gates and final authorized goldens, plus any explicit P09 dependency closure.

## Pre-edit design

The source-derived ordinary formula reduces encoded strength via C `acurrstr`, combines it with clamped constitution and caps capacity at 1000. Derive current carried weight from top-level owned arena roots using recursive container/bag weights; saturating totals are required for extreme authored weights. Core classification helpers already exist. Public pickup/take operations must check the source threshold on projected carried weight, and overloaded movement must respect C stamina/refusal behavior and actual turn cost. Whole-stack support retains the existing lack of partial stack splitting/prompts. Verify bag rounding and air/stamina conditions before integration.

Faithful tier-dependent slowdown remains an explicit P09 energy prerequisite. It must be closed before claiming the P04 burden portion of the goal complete; no synthetic movement-cost approximation is accepted. Missing effective gear, polymorph/mount/wounded-leg capacity modifiers remain separate broader contexts.

## Independent review checkpoint

The first implementation passed 17 focused tests and workspace Clippy/formatting, but fresh Sol/high source review requires repair. Attempted pickup and explicit container removal refusals must consume a normal action; their new tests incorrectly expected no turn. Overloaded movement must also refuse before confusion/stun direction RNG, unlike the initial preprocessing order. Correct the independent timing expectations and add impaired-overload public state/turn/energy/RNG coverage. The remaining scoped capacity/weight arithmetic and normal overload charge passed review. These findings remain incomplete until repaired and re-reviewed.

Three initial syntax/fixture corrections were distinct successful fixes; repair limits apply per underlying defect. The two refusal sites share one timing defect, while impairment ordering is another. Both begin their first reviewed repair attempt without escalation.

## Repaired and reviewed slice

The first repair succeeded: both selected pickup and explicit container-removal refusals charge one normal action, and overloaded movement bypasses confusion/stun direction preprocessing before its refusal. New tests compare public Wait control state/turn/energy/RNG, preserve item parents and explicitly assert the sole refusal message before comparing every remaining event. Fresh narrow Sol/high review passed and verified prior P04a/P04b hashes. Initial syntax/fixture corrections and a diagnostic event-log-control iteration are recorded separately from unsuccessful runtime repair cycles; there were no repeated unsuccessful runtime repairs or escalation.

The selected ordinary-form capacity/classification and owned bag/root arithmetic passed source review. Low-HP stamina/exercise and unsupported air/form contexts remain outside this slice; overload normal-action timing is directly verified. The [obligation record](../parity/obligations/2026-10-07-p04c.json) retains the mandatory P09 slowdown dependency. The full gate is in progress; this is not final goal acceptance.

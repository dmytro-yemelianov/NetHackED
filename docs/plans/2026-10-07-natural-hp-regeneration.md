# P05: natural ordinary-form hero HP regeneration

Queued under the active P04–P10 goal. Preserve the reviewed attribute/burden worktree. Normal Sol/medium implementer and fresh Sol/high semantic reviewer; choose Sol/high implementation only if the pre-edit timing audit requires ownership redesign.

## Required outcome

Implement one source-derived natural HP regeneration path with explicit C level/constitution/clock/state conditions and once-per-world-turn ownership. First audit actual HP storage/synchronization, elapsed turns, core fatal-tick early returns and any existing regeneration implementation. Reuse existing state; do not create a second countdown/HP owner. Pin the selected ordinary-form branch from the released C source before edits; polymorph and distinct Pw paths remain separate broader contexts.

Tests must independently establish probabilistic threshold boundaries, level/constitution behavior where applicable, full-health/no-extra-draw behavior, HP cap, fatal/status ordering, multiple hero-action/world-turn relationships and public save/RNG continuation. Source-derived expected values and prescribed draws are required where randomized healing applies. Existing golden constants remain worker-read-only; unrelated property/equipment/scheduling rewrites are excluded.

Any new serialized field needs compatible defaults and old-load tests. All gates and final authorized goldens are required for acceptance; pending global golden policy can defer commit without stopping implementation/review. Record timing ownership and explicit unsupported contexts. P09 scheduling may later extend action allocation but must preserve this once-per-world-turn contract.

## Source-confirmed design correction

Target remains `NetHack-5.0.0_Released`, commit `16ff59115315917b93185d026aeefea06db9b0f4`. An initial interval hypothesis came from older 3.6.1 behavior and must not substitute for the selected 5.0 source. Released `allmain.c:625-679` ordinary natural regeneration is probabilistic: when injured and otherwise eligible, draw `rn2(100)` once and heal one HP when level plus effective CON exceeds the draw. Full/dead/zero-HP/polymorphed cases do not use this ordinary branch. Stationary actions and represented burden conditions affect eligibility; absent Regeneration/Sleepy/prayer contexts remain explicit omissions.

Place this after fatal affliction dispatch and before hunger, preserving existing owners and early returns. A temporary actual-walking argument comes from the completed Move action and a hero ActorMoved event, distinguishing stationary attacks/blocked moves/Wait. No serialized clock or owner is added. Prescribed threshold tests replace the obsolete fixed-interval hypothesis; they must verify draw eligibility, strict probability boundaries, public save/RNG continuation and actual walking under burden.

## Reviewed implementation

Sol/medium implemented only the released 5.0 ordinary natural probability branch; fresh Sol/high review passed without confirmed defects and verified source/input hashes plus preserved P04 changes. Actor HP remains authoritative. The transient walking argument uses a hero ActorMoved event during a Move action, retaining physical walking even if a trap returns the hero to its starting position. It adds no serialized state. Fatal afflictions precede regeneration, hunger follows it, and ineligible branches avoid the healing draw.

One prescribed-draw unit and nine public tests passed, alongside thirteen existing affliction/capacity tests, workspace Clippy and formatting. Independent coverage includes strict probability thresholds, maximum/full/dead/zero/poly/fatal cases, actualwalking versus stationary/blocked actions under burden, fast/slow action-to-world-turn relationships, heal-before-hunger ordering and complete save/RNG continuation. The [obligation register](../parity/obligations/2026-10-07-p05.json) retains unsupported contexts and P09 timing integration. The initial wrong-version draft was fully removed before review; no old interval algorithm remains. Full gate and scoped golden policy are still required for acceptance.

Full integration exposed an old P03 attack-only fixture expecting HP to decrease exclusively by attack damage. Its deliberately synthetic setup now uses a zero healing threshold while retaining eligible regeneration draws; every existing damage, identity, full-save, continuation and RNG assertion remains unchanged. Fresh narrow Sol/high review passed. The rerun has 806 passing tests and all eleven other gates passing; only the frozen RUNS/COMBAT expectations fail. Input and pinned C hashes are stable. This is otherwise verified, with golden policy and final green acceptance pending.

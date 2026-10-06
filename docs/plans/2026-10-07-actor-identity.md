# P03: persisted actor species identity

Baseline: `c7ed16f`, branch `feat/monster-hp-d8`, 2026-10-07. Dependencies: [P01a inventory](../parity/obligations/2026-10-07-p01a.json) and [P02 fixture convention](2026-10-07-source-derived-hp-fixtures.md). Use one Sol/medium implementer, fresh Sol/high semantic review and coordinator-owned gates/commit under the [approved loop](2026-10-07-full-parity-agentic-loops.md).

## Outcome

Persist canonical/custom species identity on actors and populate it through owned definition factories. Species-dependent definition resolution must retain identity after a display rename, while honoring owned definitions and fixed/custom HP overrides. Confirm the minimal coherent production call-site scope before implementation; unrelated name-based behavior becomes a separate obligation.

New serialized fields need compatible defaults and load tests. Preserve old saves and explicit constructor semantics. Prove canonical and custom identity, display rename behavior, owned override resolution and save/load continuation through a meaningful public path. Expected species facts come from extracted source data; no handwritten runtime tables, dependencies or C execution. Use P02 independent fixture conventions when comparing C-derived behavior.

## Acceptance

Fresh semantic review checks identity/name precedence, missing-definition behavior, stable ordering, old loads, actual factory wiring and test strength. Full 12 gates and all three goldens must pass, tied to unchanged verified inputs. Workers may not edit checked goldens or weaken assertions. A newly required golden change outside the prior HP/item exceptions needs a concrete reviewed policy decision. Record scoped omissions, repaired findings/counters, tests and commit; this foundation does not establish full combat, generation or corpse parity.

## Bounded design

Store optional actor species provenance with canonical C symbols or a stable lowercased custom-definition key. Canonical IDs are owned by the data crate; using their generated symbols avoids a data/arena dependency cycle and follows corpse identity conventions. An absent field retains legacy name lookup; a present identity is authoritative, including when its definition is missing. Owned factories attach identity, and actor-to-definition consumers in simulation resolve through it. Polymorph and pet promotion update provenance without changing their existing stats/HP policy. Player/explicit legacy constructors can remain without identity; factory-created bones ghosts carry ghost identity.

Direct display-name quest/demon decisions, display articles and the existing pet promotion rules remain separate follow-ups. This unit changes definition resolution and provenance, not those behaviors.

## Acceptance — 2026-10-07

Sol/medium implemented optional canonical/custom actor identity through owned factories and identity-first definition resolution across the bounded simulation paths. Death corpses resolve owned actor definitions; polymorph and pet promotion update identity while preserving their existing HP/stats policy. None retains legacy lookup and omitted serialization. Unknown/removed authoritative keys cannot fall back to a different species through display names. Custom definition names serve as stable keys; renaming the definition after saving makes the old key missing.

Seven new tests in [data identity tests](../../crates/nethacked-data/tests/actor_identity.rs) and [simulation identity tests](../../crates/nethacked-sim/tests/actor_identity_tests.rs) verify canonical/custom owned overrides, fixed HP, missing definitions, corpse provenance, legacy actor/world loads and canonical serialization, renamed public attacks/stationary AI, full world/event/RNG continuation and pet promotion. Combat test definitions are synthetic semantic overrides, not C conformance claims. Existing explicit ActorRecord literals gained `species: None` as setup only; existing assertions were unchanged.

Fresh Sol/high review passed. The first full gate then caught one static ownership violation: pet-tier recognition accessed the generated bestiary directly from simulation. The first repair replaced that lookup with canonical-ID tier dispatch from the already resolved owned definition and strengthened missing-owned-target promotion coverage. Narrow independent re-review passed; no unsuccessful repeated repairs or escalation. Initial compile/lint corrections succeeded before handoff.

Final full acceptance: 780 workspace tests, all three unchanged goldens and all 12 gates, including static ownership. Complete non-documentation input hashes remained unchanged during the final run. Cargo.lock and complete golden source stayed unchanged. Evidence: `/tmp/netrust-p03-handoff.md`, `/tmp/netrust-p03-repair1.md`, `/tmp/netrust-p03-review.md`, `/tmp/netrust-p03-gates-r1/accepted.json`. Actual model token usage is unavailable. No push or PR.

Remaining direct name-based behaviors include role quest leader/nemesis/guardian decisions, achievements, genocide/class checks, shop ownership/anger, legacy shopkeeper/priest AI exceptions and display articles. Definition-level demon/undead/priest heuristics and simplified pet/polymorph rules remain incomplete. Missing authoritative definitions use existing generic combat/AI and unspecified corpse behavior. This accepts identity foundations and the declared resolution slice, without closing broad combat/generation/quest rows.

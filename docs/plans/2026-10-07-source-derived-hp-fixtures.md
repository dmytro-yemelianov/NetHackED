# P02: source-derived ordinary monster HP fixtures

Baseline: `36a315e`, branch `feat/monster-hp-d8`, 2026-10-07. Dependency: [P01a evidence](../parity/obligations/2026-10-07-p01a.json). Follow the [approved agentic loop](2026-10-07-full-parity-agentic-loops.md): one Sol/medium test writer, fresh Sol/high semantic reviewer, coordinator-owned full gates and commit. No nested agents or concurrent Cargo runners.

## Bounded outcome

Establish a reusable source-derived fixture convention for one existing ordinary `newmonhp` branch family. Expected HP values and prescribed bounded draws must be derived independently from the vendored C source, with tag, function and configuration assumptions recorded. Cover meaningful boundary cases and at least one production context. A fixture that only repeats an existing implementation assertion does not establish additional evidence.

C remains a source of facts only; no C executable or runtime. The fixture compares branch semantics under prescribed draws, without claiming same-seed equivalence between Rust and C PRNGs. Preserve fixed/custom overrides and current production behavior. No dependency, checked golden, serialized-state or gameplay changes are authorized by this unit. Any discovered production defect becomes a separate contract.

## Acceptance

Independent expected state and draw bounds, source/configuration provenance, public integration evidence and meaningful failure controls must pass fresh review. The full 12 checks and all three unchanged goldens must pass with verification bound to unchanged source/configuration inputs. Record fixture/test IDs, review findings and repair counts, model routing, remaining gaps and commit in the acceptance record. The broader HP row remains partial: `adj_lev` and incomplete plane contexts remain open.

## Acceptance — 2026-10-07

Sol/medium added [six source-derived cases](../../crates/nethacked-data/tests/fixtures/newmonhp_ordinary_v1.json), the [fixture convention](../../crates/nethacked-data/tests/fixtures/README.md), a strict prescribed-draw adapter and sampler/public named-factory checks. Cases cover zero-level endpoints, mixed two/seven-die outcomes and synthetic owned level-49 endpoints. Literal expectations include the C minimum-HP adjustment. Five source hashes pin provenance to the released tag; fresh Sol/high review independently confirmed arithmetic, adapter mapping, test isolation and upstream bytes. No findings, repair cycles or escalation.

Nine focused tests passed (three new test functions, six existing). Full acceptance: 773 workspace tests, all three unchanged goldens and all 12 gates. Complete non-documentation inputs remained unchanged throughout verification; no production, dependency or checked golden changes. Evidence: `/tmp/netrust-p02-handoff.md`, `/tmp/netrust-p02-review.md`, `/tmp/netrust-p02-gates/accepted.json`. Actual model token usage is unavailable.

The adapter deliberately depends on rand 0.9 bounded-u32 behavior; a sampler change requires adapter review, not regeneration of C expectations. Same-bound dice permutations cannot be distinguished when only their sum is observable. Tests do not dynamically rehash the C source; recorded provenance and acceptance input hashes must be checked when maintaining fixtures. This unit establishes the ordinary HP fixture convention and public factory integration. `adj_lev`, actual room/world generation and public plane reachability remain outside its claim; the broader HP row stays partial. No push or PR.

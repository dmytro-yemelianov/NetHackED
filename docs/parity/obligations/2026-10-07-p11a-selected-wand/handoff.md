# P11a-S1 implementation handoff

Backend/native/shared-parser implementation is ready for coordinator review. No commit, progress/ledger/docs mutation or family acceptance claim. Graph tools were unavailable; bounded known-path fallback used. Exact six file paths and SHA-256 hashes are in `handoff.json`.

## Changes and backend evidence

- `ZapSelectedWand { item_index:u32, expected_item:Option<SlotId>, dir, energy }` with default/omission serde, and explicit unit `CancelZapSelection`.
- Complete direct-carried snapshot index validation precedes captured generational-token equality, class and owned-definition resolution. Refusal/cancellation never spends energy. Selected and legacy zap share the unchanged charge/effect resolver.
- JSON selected parsing rejects absent/null index, malformed/negative/fractional/out-of-u32 values before narrowing, malformed/out-of-bound/noncanonical token, and invalid direction. Cancellation rejects any nonnull declared argument while retaining player context. East6/default and legacy index-ignore remain.
- Ten native regression tests cover S01/S02/S03/S05/S06/S07; two new commands unit tests cover shared-parser S04. Detailed exact fixture names and evidence are in JSON. S01 verifies selected create2→1 while first fire7 remains untouched, counts1/2/8 against accepted independent draw/placement/HP/allocation oracle, scheduler/event order and save continuation.
- No-time tests reconcile a living ready hero, then compare complete canonical saves and native RNG allowing only event_log. Container/floor/moved/deleted/recycled/stale tokens, class and unresolved identity produce exact reviewed messages. Witness actors/terrain remain unchanged. Helpless/dressing cancel/selected actions still match Wait replacement.
- Canonical/custom owned overrides, display rename, legacy omitted definitions/None fallback, old Zap JSON bytes, optional token omission, selected/cancel typed transcripts, native pre/post save/load and continuation are covered. Signed/zero/cursed/blessed behavior is compared exactly with single-record legacy behavior, as compatibility only.

## Exactly three existing full-literal setup changes

1. `commands.rs::action_args_from_json`: initializes new expected_item from validated selected context, None for other actions.
2. `commands.rs::tests::every_mcp_step_action_parses`: adds expected_item:None only; existing inputs, loop and assertions retained.
3. `graphql.rs::step`: adds expected_item:None only.

## Validation

Exact focused commands, exit codes and log paths are in JSON. Native focused suites pass (6 identity +14 creation +10 selected); core/arena/data pass; WASM22 pass; Python check passes; pack40 pass. Agent34 unit tests pass, then the exact package command exits101 on five existing bones HTTP tests at `bones_hardening_test.rs:13:71`: TCP bind returns PermissionDenied/Operation not permitted. Later package integration tests are unexecuted by that command; coordinator must rerun in a socket-enabled environment. All three frozen golden determinism tests pass, including the longer vanilla runs fixture (see `07-golden.log`). Allowed-file rustfmt and diff whitespace checks pass.

Both original new-fixture failures and repairs are retained in `01-sim*.log`: fixtures incorrectly assumed insertion order after arena slot reuse; repaired by assigning records to captured actual snapshot IDs and explicitly shifting the captured index. No gameplay expected values were weakened. A mistaken overlapping core/WASM Cargo launch occurred; both exited0, then exact commands were rerun individually with exit confirmation in the `*-sequential.log` logs. Initial uv cache permission failure was repaired using task-specific UV_CACHE_DIR; all Python used the required uv/local interpreter. `ps` was sandbox-denied; launched sessions are tracked via tool completion.

## Unchanged and successors

Old assertions/ZapWand constructors, legacy None identity, owned resolver, pack schemas/hashes, default numeric22/Discrete26, dependencies and frozen goldens are untouched. No STEP_ACTIONS, protocol/frontend, factory/ItemRecord/ItemDef changes. Coordinator-owned dirty autonomous ledger is untouched.

M owns every route inventory/index/token/schema adapter, enum-test adaptation and independent JSON-RPC library/binary parser-envelope evidence. MC owns all post-D1 machine charged-continuation transcripts. F owns TUI/both-browser interactive flows and consumes all-route MC evidence. D1 native pending/charged continuation, C1 charge/wrest/dust, B1 backfire and P1 preflight remain successors. No all-ten completion, broad family closure or source charge-conformance claim. Fresh independent implementation review, full12 gates/digest and any commit belong to coordinator.

All worker-launched processes have exited. No detached processes. Unrelated host process inventory was unavailable because ps is sandbox-denied.

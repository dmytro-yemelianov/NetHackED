# Signed wand-charge API prerequisite

P11a-C1-API1 adds a pure signed-charge decision API. Positive charges decrement once without a draw; negative charges remain unzappable. Zero charges require a prescribed draw in 0..120: zero yields a wrested ready result at -1; other draws leave zero unzappable. Missing, out-of-range or unexpected draws return typed errors. The existing unsigned charge and recharge APIs remain unchanged.

The shared native handler uses the primitive for existing positive charges only. It preserves the selected item generation, refusal order, effects, RNG and turn behavior. Native zero/negative behavior remains unchanged until IN1 and its reviewed prerequisites are accepted. This checkpoint does not activate wrest, dust or backfire.

The [independent review](review-final.json) passes without findings. [Public fixtures and focused results](handoff.json) cover selected canonical/custom positive boundaries, recharge preservation, active/cached signed values, old saves and native continuation. Existing tests and helpers remain unchanged. Two failed fixture runs and their source hashes are retained in durable evidence; the final fixtures pass.

[Unit evidence](unit.json) binds all345 source inputs and fresh1,026 passing tests, all3 unchanged goldens and all12 passing gates. Final documentation checks and a verified delivery commit complete acceptance. The [reviewed contract](../2026-10-07-p11a-charge-contract/README.md) and [run ledger](../2026-10-07-autonomous-parity-run.json) retain billing, lifetime, exercise, half-physical damage, backfire, preflight, nonpositive integration and frontend successors. All53 families remain open.

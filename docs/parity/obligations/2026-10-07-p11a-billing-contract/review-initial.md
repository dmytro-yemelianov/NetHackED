The contract needs three bounded repairs before registration. Its retained-debt model follows the pinned C branch, and U3 is resolved by the coordinator manifest. This verdict concerns contract readiness only; it accepts no billing implementation or family closure.

F1: Call record_bill_terms twice for the same valid full-generation unpaid item, with identical or changed terms.

contract.json api.admission defines item/ledger validation but gives no existing-receipt result. representation.invariants permits at most one receipt, while representation.liability freezes terms at admission. Replacing the existing entry would satisfy uniqueness while changing frozen liability.

Repair: Define both repeated-admission outcomes explicitly. Reject changed terms atomically; choose rejection or exact idempotence for identical terms. Specify the error/result and add complete-state/RNG/events/time comparisons. Permit no receipt replacement under this child.

F2: After successful receipt admission, invoke existing remove_unpaid directly or reach it through Pay.

world.rs:680 removes the compatibility tuple only. economy.rs:117–140 calculates current projected prices and calls remove_unpaid. The proposed API1 allowlist expressly preserves that body, yet global live_terms requires a corresponding unpaid owner and SAVE1 rejects dangling receipts.

Repair: Specify and enforce a bounded policy for existing ledger-clearing paths while live receipts exist. Either refuse unsupported settlement atomically pending DG06-bill-settlement, or separately authorize a source-backed synchronized route. Enumerate any additional existing body before implementation approval; keep consumed debt untouched. Add admission→clearing/Pay→view/save/disposal coverage without expanding into a full shop audit.

F3: Root prepares BILL-SAVE1’s required oldpack/schema/hash acceptance evidence.

compatibility.pack_schema_hash assigns exact fixture IDs and scratch comparisons to SAVE1 but leaves the generation manifest for later root collection. validation.focused_future_commands supplies package tests without concrete schema-generation, canonical-hash or scratch-comparison argv.

Repair: Supply the minimal SAVE1 fixture IDs, exact generation/hash/comparison argv, scratch destinations, expected unchanged outputs and failure exits. Assign the focused commands to API1 versus SAVE1. Keep production outputs and every existing assertion/setup frozen; no execution is required for this repair.

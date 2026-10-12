# P04b: ordinary hero strength damage bonus

Baseline: `b912faf` with the reviewed P04a worktree preserved, 2026-10-07. The active user goal is completion of P04–P10 under the approved agentic loop. Use one Sol/medium implementer and fresh Sol/high semantic reviewer. Coordinator owns policy, full gates, obligations and commits.

## Outcome and boundaries

Connect the existing stored hero strength to the ordinary melee damage contribution from released C `weapon.c:dbon`, preserving weapon/enchantment/skill contributions, hit/miss draw order and owned defender definitions. First confirm the actual production damage path and representable encoded strength, existing minimum-damage handling and C call-site ordering. Damage, hit and burden remain separately attributable acceptance units; preserve the already reviewed P04a contribution. Equipment/temporary effective attributes and distinct polymorph rules remain explicit later obligations unless a direct prerequisite requires a bounded decision.

Independent literal C boundary cases, a public attack regression that detects bypassed damage contribution, owned/fixed-HP behavior and save/RNG continuation are required. New schemas/dependencies/C execution and assertion weakening are excluded. Existing-test adjustments must be setup only and listed.

Workers cannot edit checked goldens, commit or change coordinator documents. Pending golden policy does not prevent implementation or review; acceptance/commit still requires all full checks and final authorized goldens. Preserve unrelated worktree changes and record exact added diff, source/config assumptions, test evidence and remaining omissions.

## Reviewed implementation

Sol/medium added the ordinary stored strength contribution and combined it with the existing skill term before the final damage floor, preserving hit-only damage draws and the reviewed hit formula. Fresh Sol/high review passed without confirmed defects. Independent fixtures cover 21 strength/clamp cases; public regressions cover 256 combinations of owned canonical/custom identities, renamed fixed-HP actors, direct/bump attacks, barehands/enchantment/skill, hit/miss boundaries, damage floor, full save and RNG continuation, plus polymorph exclusion. Bypassing the contribution makes the public test fail; restored source passes.

The uncommitted P04a public matrix now includes independent literal expected dbon values without weakening assertions. Four committed dice tests retain all assertions and gain neutral STR10 setup in their shared helper: dagger and long-sword small/large targets, non-monk barehands, monk barehands. All 520 core/simulation tests passed. Effective equipment/temporary STR, twohand/dual scaling, rings and broader polymorph damage remain explicit separate contexts. Full gate and golden policy acceptance remain pending.

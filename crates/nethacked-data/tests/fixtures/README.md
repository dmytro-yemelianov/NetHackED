# Source-derived fixtures

`newmonhp_ordinary_v1.json` is the first fixture of this convention. Schema
version 1 records an evidence kind, upstream tag and hashed source references,
explicit configuration/branch assumptions, and cases with independent expected
state and a human-readable arithmetic derivation. Expected fields are literal
source-derived values; never regenerate them by calling Rust production code.

Each case's `draws` is an ordered list of blocks. A block supplies `bound` and
chronological zero-based `values` for bounded core RNG outcomes. These describe
C `RND` results (`d` calls it directly), not a C seed or literal `rn2` call trace.
Expand blocks in order. The HP branch uses only bounds 4 and 8; the test adapter
in `support/source_derived.rs` intentionally supports only those bounds.

The adapter places each outcome in the middle of its rand 0.9 u32 multiply-high
bucket. It fails on extra, skipped, byte, or u64 requests. Expected HP detects a
wrong bound or value interpretation. Reordering draws within one uniform-bound
dice sum is observationally equivalent; this fixture does not claim to detect
that permutation. There is no same-seed Rust/C comparison or C execution.

Cases cover zero-level endpoints, mixed two/seven-die outcomes, and both ends of
the last ordinary definition level (49). Level-49 inputs alter only the owned
test definition and are synthetic boundary cases, not unmodified giant-ant C
records. C `m_lev` is stipulated equal to the definition level; `adj_lev` remains
an open parity gap. `sample_hp`, custom/fixed policy, and plane reachability are
not changed. The public named Ruleset factory checks HP, maximum HP, level,
coordinate, and exact draw consumption against these independent literals.
Actual room/world generation is separate evidence in existing tests.

# Owner persistence D1

This crate implements the structural `OwnerSaveV1` wire and `SaveCatalog0`, at
contract HEAD 7b4e74d. It does **not** establish persistence acceptance. D2–D6, full A5,
M5–M7 and full parity remain open. It has no kernel/World dependency, filesystem
operations, factories, execution, restoration, code loading or automatic migration.

```rust
use nethacked_persistence::{decode, encode, SaveLimits, OwnerSaveV1, Error};
fn transport(bytes: &[u8]) -> Result<OwnerSaveV1, Error> {
    let limits = SaveLimits::default();
    let save = decode(bytes, limits)?;
    assert_eq!(encode(&save, limits)?, bytes);
    Ok(save)
}
```

The 52-byte header is `NHOWNR01`, little-endian u32 envelope version 1,
little-endian u64 body byte length, and SHA-256 of the exact body. Body JSON
uses UTF-8, lexicographically sorted object keys, no whitespace, unsigned u64
integers, lowercase hexadecimal bytes and 32-byte artifact digests. Strings use
serde_json's minimal escaping (Unicode scalars stay literal). Unknown and duplicate
keys are refused at every typed level. JSON booleans/null express flags/options.
Format/profile versions are both 1; these are not save-controlled limit profiles.

IDs consist of `IdKind` and nonzero u64 serial. Kind order is the public enum's
declaration order; primary tables sort by ID. Tombstones/retired IDs use that
same order; counters sort by kind; publication domains sort by native issuing host; consumed tickets sort by issuing
host/ticket;
current dispatch sorts by owner ID. Secondary records (migration evidence,
retention edges, ordinals) are typed metadata rather than independent runtime IDs.
Semantic arrays retain their exact order: import slots, routes, ledger, events,
trace, participants and timer ties. The encoder refuses unsorted primary tables;
it never sorts semantic arrays. Multiple references may share a single reservation
row. Retired serials cannot overlap live rows; tombstones identify historical
origins. Every used/retired ID is dominated by its kind's inclusive high-water
counter. Holes below high water are unavailable for future issuance. Publication ticket high-water evidence belongs to the native owner host, separately
from operation/workflow sequence. It persists in `publication_domains` after
operation disposal; tombstones retain operation checkpoint evidence. Domain zero,
exhausted counters, zero/out-of-range tickets, duplicate pending use across batches
or operations, and pending/consumed collisions refuse. Consumed history may share
its matching history receipt, but no pending ticket. Domains are saved identities,
not authority; approved host reconstruction must verify and remap them.

The typed catalog includes exact module artifact/kind/ABI, manifest configuration,
ports, extension and import slots, declarative grant requests and current dispatch;
independent owner incarnation/generation/revision, exact schema/backend/codec/policy
identities, ports/dependencies and bytes; operations, leases, pins, callbacks,
shared reservations; candidates, batches, publication receipts/events/cursors;
workflow program binding, phase/sequence/status, trace, tickets/prefixes/cleanup;
input source/policy/cursor bytes, ordered ledger, ordinals, request/value/provenance
and observations; timers' task/profile, deadlines/ties/residency/status/delivery;
retention, counters, retired IDs, consumed tickets, tombstones and exact migration
source/target identities. Identity means name/version/artifact, never a resolver
or runtime handle. Required reference categories are explicit fields, not hidden
inside an unnamed all-state blob.

Publication receipts expose caller/artifact, operation/lease/checkpoint/phase,
coordinator capability/resolution/effective scope, policy, ordered participant
receipts (stamps, commands, behavior, digests and recorded-input identities),
recursive readonly receipt evidence, events, replies and diagnostic history bytes.
Prepared batches separately declare required writes and effect order; candidate
membership never supplies these declarations. Live readonly/prepared observations
also expose retained code, behavior revision, helper authority closure and execution
bindings. Named native capability/resource labels remain exact strings; associated
resolved modules/owners/inputs are separate typed graph edges. Policies/accounting
identities are declared independently of their private codec bytes.

Workflow phase is the native string. An initial Running frame has an explicit null
issued ticket. Issued tickets carry operation, sequence, lease, pipeline digest and
phase. Suspended frames require a matching ticket; resumed Running frames retain
the earlier issued evidence. Sequence/checkpoint and reference validation apply
whether or not a ticket exists. This metadata cannot resume an operation itself.

Private owner, prepared-state, service/program/provider/task and adapter payloads
remain opaque hex bytes. D1 does not validate their private semantics or prove that
an opaque codec's internal edges have been enumerated. D2–D4 must use approved
owning codecs to decode, enumerate/remap internal references and validate old
state, stamps, full retained closures, program semantics, policy/accounting and
authority. D5 provides explicitly approved migration/compatibility; D6 establishes
composed acceptance. Structural metadata and declared outer edges are checked now;
private payload bytes cannot supply omitted outer metadata or grant permission.
Saved artifact digests, policies and grants are untrusted declarations. SHA-256
checks corruption only. Historical source/origin/operation/prefix/retention edges
may reach explicit tombstones; required live edges cannot. No pointers, credentials
or authority are reconstructed by this crate.

Default hard ceilings are 64 MiB complete wire, 16 MiB individual decoded payload,
65,536 entries per table/semantic array, 262,144 aggregate reference edges, 256
UTF-8 bytes per name/string and nesting depth 32 (root counts as 1). Retention arrays
use the edge ceiling. Hosts may only narrow these; key names have a fixed 256-byte
syntax ceiling independent of caller name bounds. Hex overhead counts against
wire size. Header length conversion, addition and slice bounds precede body access.
A bounded syntax/count walk checks UTF-8, depth, array counts, raw payload
size/hex and decoded name sizes before typed table allocation. Bounded key/name
scratch strings (including the previous key at each nesting level) are allocated
during this walk; hex payloads allocate no scratch.
Typed serde parsing and a temporary bounded structural map follow that pass.
Encoder uses a capped writer before constructing its temporary canonical sorting
map. All exposed hex decoding validates size/syntax before reservation. Limit
failures leave callers' immutable values untouched. There is no production unsafe.

Errors distinguish envelope `Truncated/Magic/Version/Length/Digest`, syntax
`Json/Noncanonical/Schema`, bounds `Limits/Depth/String/Payload/Rows/Edges`,
`Hex`, graph `Id/Counter/Reference`, and failed explicit output `Allocation`.
Several malformed inputs can violate multiple rules; framing, digest and syntax
precede graph validation. Rust allocator exhaustion in serde is not promised to
be recoverable; all its attacker-dependent input sizes have finite preflight caps.

`tests/fixtures/{empty,graph,host-ticket,initial,multi-owner}.{json,wire}` are independent Python JSON/hashlib/struct
vectors, not output of the Rust encoder. The graph contains all table categories,
a disposed historical source, two callbacks sharing one reservation, consumed
and pending tickets, retained code, ledger and intentional nonlexical semantic
orders. Reproducible Python generation source, the original independent seed and revision
provenance are in `tests/fixtures/{generate.py,seed-graph.json,provenance.json}`.
Original fixture revisions, all five writer histories, baseline-red results, repair
counters, before-check snapshots and raw logs/exits/hashes are archived under
`/private/tmp/MICROKERNEL-OWNER-WIRE/repair1-evidence`. No existing goldens changed.

The schema preserves the existing manifest's endpoint major/access/request/reply
bounds, ordered capability slots, extension points and target/export declaration;
owner ports carry access/major and typed owner/query/schema dependency declarations.
Scopes are declarative `All` or typed owner/key requests, never restored grants.
Owner and stamp module bindings are explicit outer references. Artifact digests
have a fixed 32-byte bound independent of the caller's private-payload ceiling.
All option fields require explicit nulls. Every live owner has exactly one current
dispatch row. The reviewed [D1 contract](../../docs/plans/2026-10-08-owner-persistence-contract.md)
and its existing O1–O4/acceptance references remain the acceptance basis.

# nethacked-owners — M4-O2 / M4-O3

Optional, entity-free owner storage, preparation and publication. The default build depends only on
`nethacked-kernel` and SHA-256, with no NetHack, World, resource service, or entity
imports. Each `Rc<Owner<T>>` physically owns one private `StateSlot<T>`; unrelated
Rust types and schema-bound blobs coexist. Private `RefCell` guards let the sealed
runtime move values; no slot borrow escapes a port. `Host` stores IDs, generations, schemas,
module metadata and weak opaque routes, including permanent tombstones. It has
no authoritative payload map. One module/domain implementation serves many instances.

`Host::enroll` is privileged initialization, validating the schema and encoded
payload before issuing a non-reused host/owner identity. `Backend::CommitCallback`
refuses. `Schema`, `NativeBehavior` and `WasmCodec` are trusted bounded host adapters:
validators, encoding, decoding, queries and native preparation must be pure,
side-effect-free and free of interior/shared mutable aliases. Encoded size must
faithfully bound retained representation. Traits cannot establish these premises.
CPU bounds and unwind safety are host obligations; native panic propagates and
requires host fail-stop, not a clean-refusal claim. Guest bytes never implement
codecs or choose generic proposed states. Blob preparation uses a declared Wasm
algorithm and trusted invariant validation, rather than a native mock.

Named ports bind ABI major, access and an exact ordered dependency declaration.
Resources are scoped to a particular host, owner identity and port. A host wires
owners into registered `NativePort` callbacks; those callbacks use `Owner::query`
and `Owner::prepare` with the actual kernel `CallerView`. Guest credentials are
never accepted. Admission checks provider artifact, authenticated caller artifact,
requested capability, effective resource scope, owner schema/incarnation and gate.
Query owners require their own Read grant even when absent from observations.
Dependencies additionally validate their captured owner/query/schema/revision,
caller, exact resolution and live grants, recursively through derived observations.
Detached `CallerView` snapshots cannot query, prepare, revalidate or call helpers.

Queries return immutable detached bounded `Observation` bytes, a stamp and private
code/route/dependency captures. `Owner::inspect` is privileged bounded detached
encoding under the same gate; no state reference escapes. The host uses a serialized
single-thread executor (`Rc`, `Cell`), with fallible gate entry. Reads, preparation,
registry inspection, current stamps, pending counts, history, enrollment and retirement
share that gate. O3 publishes under this exclusion; no threaded memory-order claim is made.

Native domain behavior computes `Proposal<T>` from its own immutable state, request
and exactly declared detached observations. Wasm codecs supply this same bounded
input to `CallerView::call_pure` on the provider's declared helper slot. The bridge
uses shared fuel/calls/depth, current plus retained grants/routes, and a forced Read
ceiling. Every descendant native dispatch refuses before callbacks, including Read
services. Wasm helpers and extensions may call other authorized Wasm algorithms.
No fresh kernel invocation or live host reads occur during guest preparation.

`Candidate<T>` privately owns the fully constructed typed next state, exact events
and staged reply; it retains the owner, dependencies, invocation code and actual
helper chain. Its receipt binds stamp/schema, named command/capability, behavior,
caller/artifact, full operation/checkpoint/phase/lease and request/next-state digests.
`Candidate::validate` still requires a fresh view at its own provider.
`Candidate::erase` supplies sealed heterogeneous proposals to `Host::batch`.
There is no replacement-state setter, implementable publication trait or native
commit callback. Owner-produced proposals remain the sole command mutation path.
`discard`/drop frees staging; typed cleanup also clears staging held by external
candidate handles. Preparation requires the host cleanup identity in the authentic
operation binding. Missing enrollment refuses before behavior.

Default limits: 128 owner identities including tombstones, 64 pending candidates,
16 ports per owner, 32 transitive observations, 16 KiB each for encoded payload,
aggregate observation bytes, aggregate event bytes and reply bytes, and 32 events
per candidate. Requests and helper input/output also obey payload limits; kernel
endpoint and VM limits apply independently. Owner/candidate/pin/kernel counters
refuse before overflow. No identity reuse, eviction or implicit counter reset.
Host-issued code pins protect owner schemas, observations and pending work against
both unload APIs; last drop releases pins. Pending candidates prevent retirement.
Retiring and enrolling creates a fresh identity and leaves the tombstone behind.

## O3 publication and cleanup

The trusted host declares a required write set, profile event order, readonly
observations and a policy identity plus pure `Accounting` validator. Validation
sees bounded detached before/proposed encodings, receipts, events and replies;
it cannot write slots. Domain/profile wiring must declare the complete accounting
set and implement its conservation laws faithfully. An arbitrary permissive
validator does not establish a domain conservation theorem. Native validators,
codecs, adapter/code correspondence, encoded representation bounds and absence of
mutable aliases remain explicit trust assumptions. The runtime cannot prove them.

`Host::batch` captures a fresh coordinator Write capability and invocation code;
it issues an opaque monotonic ticket. `Batch::commit` closes one common host gate
and checks fresh coordinator authority, every participant's current named Write
permit/resolution, caller/artifact, exact operation/checkpoint/phase/lease, live
owner/schema/revision, readonly and transitive grants/stamps, proposal digest and
retained code. This separate coordinator path does **not** relax owner-provider
checks on query, prepare or `Candidate::validate`. Foreign gates, duplicates and
inexact declarations refuse. Compatible future dispatch changes retain valid
captured chains; changing the pending command's captured chain is unsupported.

All invariant/accounting callbacks, encoding, aggregate resource/counter checks,
complete receipt/reply/history construction and history reservation finish before
publication. Canonical OwnerId order acquires **every** typed slot/staging guard.
A sealed linked guard stack then moves payloads into existing retention cells,
updates revisions, the complete profile-ordered event prefix, pending counters,
ticket consumption and epoch. It invokes no native/guest adapter, codec or owner
finalizer, and allocates/deallocates/drops no payload/token. Registry metadata
also advances before the gate opens. All public authoritative reads and reentry
refuse while gated; detached old observations never become live authority.

Refused commit preserves slots, history, pending reservations and ticket validity
for retry. Discarded/consumed tickets cannot publish again. Old payloads, staging
observations and code tokens survive until opening, then release. An old native
destructor panic resumes unwinding **after** setting permanent host quarantine;
it is never a clean refusal, replay, rollback or implicit reset. Live ports refuse
in quarantine; `quarantine_history` supplies only diagnostic committed receipts.
A second destructor panic during unwinding can abort the process. Allocator abort,
runtime bugs and process crash are outside this in-memory atomicity guarantee.
A host may contain a prepublication panic only under established native purity
and unwind safety; arbitrary native preparation panic retains the fail-stop rule.

Default publication limits: 128 lifetime tickets/commits, 64 participants, 4,096
history events, 1 MiB history storage, 1 MiB aggregate retained staging/old-state
encodings, and 1 MiB cumulative replies. Per-commit event/reply/observation limits
also apply in aggregate. `configure_publication` is allowed only before staging
or ticket issuance. No eviction, ticket/owner ID reuse or counter reset exists.
Staging reservations conservatively include each candidate's maximum observation
bytes; faithful encoded size/alias assumptions bound native retained representation.
History has fully constructed typed records and a bounded version-tagged Debug
diagnostic encoding; replies are ticket+epoch then length-prefixed participant
bytes in profile order. These are not owner restoration or durable journal codecs.

Enable `workflows` for `CleanupParticipant for Host`; enroll `Rc::new(host.clone())`
in the actual `Workflows::start` participant list. This optional dependency may
transitively link the existing service crates, but requires no resource-service
instance, entity, World or NetHack integration. Phase exit, rejection/cancel and
trap call `discard_operation` on the exact binding. Cleanup prechecks all staging
borrows, preserves staging when busy, invalidates held external candidates and
releases their code/owner/dependency references after opening. Retry is idempotent
cleanup only, preserving previous commits. A foreign checkpoint cannot adopt
staged work; foreign-host staging remains the foreign cleanup participant's job.

## O1 correspondence and fixtures

| O1 definitions / witnesses | O2 concrete evidence |
| --- | --- |
| `RegionLayout`, `separateRegions`, `swap_region_frame` | `heterogeneous_storage_native_and_actual_wasm_candidates_are_independent`: separate inventory allocations, independent ledger and blob, metadata-only registry; actual native and guest candidates, unchanged live bytes. No swap refinement claimed. |
| `Domain.legal/propose`, `prepare_preserves_live`, `prepare_refusal`, `Witness.EntityFree` | Native inventory/ledger algorithms and blob `(state + observation) × request`; invariant/malformed/oversized/trap/fuel and native bound refusals preserve slots and candidate counters. |
| `preflight_observations`, `invalid_readonly_refused`, `Witness.QuerySafety` | Own query grant independently revoked/narrowed while dependency grant survives; dependency and transitive revocation, captured stamp/schema/caller corruption, retired/recreated identity and common gate refusal. |
| `wrong_invocation_refused`, `preparation_serial_advances` | Candidate validation binds caller, operation/phase/checkpoint/lease and current grants; detached capture refusal, finite staging/counter reservations and discard. |
| `prepare_pins_chain`, `pending_chain_unload_refused` | Owner/candidate/observation RAII pins; retirement refusal while pending; code unload refusal until last reference drops. Kernel retained-route fixture preserves captured helper chain while future dispatch changes. |
| Immutable guest preparation premise | New kernel `pure_preparation` fixtures: actual computation, native Read-import exclusion, guest nesting, shared fuel/calls/depth, detached bridge refusal, current/retained grants and executable routes. |

O3 implements the publication protocol below. O4 now has dynamic owner,
derived-cache and optional [service composition fixtures](../nethacked-owner-services/README.md),
pending independent integration acceptance. M4d must accept
codecs/restoration. NetHack migration, crash recovery, independent semantic review
and aggregate ownership-runtime acceptance remain unfinished.

## O3 evidence against the retained O1 map

The [ownership proof map](../../docs/ownership-proof-map.md) and
[contract](../../docs/plans/2026-10-08-autonomous-ownership-contract.md) remain the
obligation sources. These fixtures provide runtime evidence, not a complete Rust
memory refinement, native-purity proof, crash recovery or O4/M4d acceptance.

| O1 obligation | O3 fixture / evidence |
| --- | --- |
| `PublicationStep`, `no_partial_observation`, `opening_requires_completed_metadata`, `publication_authority_commit` | `heterogeneous_transfer_excludes_every_read_between_swaps_and_publishes_profile_prefix`: independent Stock/Item types/providers, recorded intermediate revision vectors `[1,0,0]`, `[1,1,0]`, `[1,1,1]`; every authoritative read/reentry refuses; profile event order differs from canonical guards. Test-only allocator/codec/callback/drop probes measure zero in publication. |
| `Witness.Transfer`, `Witness.Retirement`, `accounting_rejection`, `refused_retirement_keeps_charge_prefix` | Transfer plus `debt_retirement_requires_accounting_owner_and_preserves_terms_and_prior_transfer`: omitted debt refuses, quantity becomes consumed quantity, terms/creditor/liability persist, earlier accepted prefix is exact. These are host examples, not the complete selected-wand/equipment/timer slice. |
| Authority/refusal/replay / finite bounds | `refusal_matrix_preserves_slots_prefix_pending_and_retryable_tickets`, stale readonly/cleanup, duplicate/omitted, foreign-gate/missing-enrollment and foreign-caller fixtures: authenticated failure markers, exact snapshots/counters/history, two retries and zero swap-probe invocations on refusal. |
| `prepare_pins_chain`, `compatible_future_current`, entity-free storage | `actual_wasm_proposals_commit_heterogeneous_state_and_retain_old_dispatch`: real guest proposal bytes and imported commit, changed live helper dispatch demonstrated by a fresh guest call, retained old-chain batch publication; independent inventory/blob slots, no entities. |
| Failure boundary / committed prefix / native destructor premise | `contained_pure_preflight_panic_preserves_retryable_proposals_and_no_swap_occurs`, `old_payload_drops_after_opening_and_panic_quarantines_committed_receipts`, `committed_wasm_call_then_reply_error_or_trap_keeps_exactly_one_prefix`: contained pure preflight panic, post-open destructor marker/quarantine, committed-prefix diagnostic receipt and refusal of replay. |
| `workflow_prefix_composition`, `workflow_reject_prefix` | With `workflows`, actual workflow phase advance/trap/busy cleanup/retry/revocation and cancellation fixtures; held candidates lose staging, prior commits persist, terminal cleanup cannot replay. Root code unload succeeds while cleaned opaque candidates remain held. |

### Exact O2 fixture setup/accessor deltas

`Harness::new` and `Harness::refresh` enroll the host cleanup identity using
`operation_binding_with_cleanup`. `dep` unwraps the gated detached stamp.
All existing `pending` assertions unwrap the gated count without changing their
expected numbers. Private slot assertions/pointer comparisons acquire immutable
`RefCell` borrows; private next/event/reply/code assertions read the shared staging
cell; the receipt assertion uses its readonly accessor. Existing domain algorithms,
expected values, predicates, assertion order and fuel-marker control are unchanged.

Affected existing tests: `heterogeneous_storage_native_and_actual_wasm_candidates_are_independent`,
`stale_foreign_recreated_and_detached_captures_refuse`,
`invariant_malformed_oversized_trapped_and_fuel_refused_guest_candidates_preserve_state`,
`bounded_candidates_identity_counters_and_code_lifetimes`,
`native_refusal_all_staging_bounds_and_counter_exhaustion_preserve_slots`,
`observation_closure_bounds_and_transitive_dependency_revocation_are_checked`,
`candidate_revalidation_binds_operation_phase_lease_and_live_grants`.
`query_owner_grant_is_required_even_when_owner_is_absent_from_dependencies` has
no assertion/body delta; it receives the shared cleanup-enrollment helper change.

## M4d-D2 independent state persistence

Enable `persistence` for the host-only `persistence` module. It adds optional serde
and serde_json using existing workspace versions; the default dependency graph
stays unchanged. Owners do not depend on the persistence orchestrator. `Checkpoint`
is a local bounded snapshot, adapted to the accepted D1 envelope by that orchestrator;
this module does not implement another OwnerSave transport or grant authority to bytes.

`PersistenceCodec<T>` is separate from `Schema`, `NativeBehavior` and `WasmCodec`.
Trusted host adapters bind exact schema/codec/policy names, versions and artifact
identities, perform bounded state encoding/decoding and semantic validation, declare
internal live/historical references, and approve exact ports, dependency declarations
and query policy. The native typed, actual Wasm-backed blob and private field-buffer
implementations in `tests/persistence.rs` demonstrate independent reconstruction and
real post-restore preparation, query and publication. Purity, bounded CPU/allocation,
codec fidelity, absence of mutable aliases and effect-free fresh factories remain
explicit host trust premises; these traits cannot prove them.

`Host::capture_state` holds the common visibility gate across every owner-local
slot encoder. Every live owner must be captured exactly once. `Restore` creates a
hidden fresh host, remaps checked live references, owner dependencies and resource
strings, and invokes host-provided fresh backend factories bound to approved kernel
modules. The host resolves exact artifacts independently; saved digests cannot
issue grants. Old bytes decode and validate semantically before remapping or pure
migration; the exact old codec is mandatory even when a migrator exists. Transformed
state and configuration validate again before enrollment. Schema changes referenced
by another owner require a graph migration plan and currently refuse explicitly.

Owner generations/revisions, tombstones and owner/candidate/ticket high-water
counters persist. Restarts allocate new owner host namespaces excluding every admitted saved origin,
including disposed historical namespaces, and fresh cleanup identities. Allocation
skips at most the bounded origin count, uses checked increments, and never walks to
a supplied maximum;
retired serials and consumed/abandoned tickets cannot be reused. Historical namespaces
stay explicit across successive restarts (bounded by the owner identity limit).
`Host::persistence_history` returns the complete historical publication prefix plus
new receipts: events, replies, epochs, participant revisions/candidate serials and
complete original diagnostic origin encodings. Typed ordered receipt trees preserve
stamps, candidate IDs, policies, observation metadata and participant event/reply
contributions. Validation reconciles every duplicated publication field and the
complete diagnostic encoding, and enforces continuous per-owner revision lineage
through retirement and repeated restores. Historical schemas remain associated with
their original namespaces across supported migration. Opaque kernel credential
interiors are retained only as diagnostic leaves; they are never parsed or rebuilt.
Metadata is untrusted consistency evidence, not authenticated authority or a
tamper-proof save. These origins are historical evidence,
never reconstructed operation bindings or permits. `Host::history` continues to return
only this runtime's typed receipts; authentic retained kernel bindings are D3's seam.

Hidden owner handles refuse while the restore gate is closed. Incomplete/failed
construction quarantines those handles when dropped; dropping a failed builder cannot
activate escaped owners. A successful `finish` checks the complete live dependency
graph before opening. This is local host construction, not D4's destination authority
validation or atomic bundle activation. Source and nonempty destination hosts are
never used as hidden storage and remain unchanged on refusal.

Pending candidates, live unconsumed batches, captured behaviors, compatible retained
helpers and recorded declarations currently return `Unsupported`; none are silently
omitted. Complete retained kernel/prepared state belongs to D3, service state and
activation to D4, native compatibility to D5, and combined A5 acceptance to D6.
This local seam does not close D3–D6, A5 or M5–M7, and supplies no native save migration
or crash durability guarantee.

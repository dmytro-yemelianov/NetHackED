# nethacked-owners — M4-O2

Optional, entity-free owner storage and domain preparation. It depends only on
`nethacked-kernel` and SHA-256, with no NetHack, World, resource service, or entity
imports. Each `Rc<Owner<T>>` physically owns one private `StateSlot<T>`; unrelated
Rust types and schema-bound blobs coexist. `Host` stores IDs, generations, schemas,
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
registry inspection, enrollment and retirement share that gate. It is preparatory
exclusion infrastructure, not O3 atomic publication or a threaded memory-order claim.

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
`Candidate::validate` provides staging checks with a fresh authenticated view.
`discard`/drop frees the pending reservation and references. There is **no public
apply/commit** and no slot mutation API. Live state/revision never changes in O2;
events remain staged, and there is no live event history yet.

Default limits: 128 owner identities including tombstones, 64 pending candidates,
16 ports per owner, 32 transitive observations, 16 KiB each for encoded payload,
aggregate observation bytes, aggregate event bytes and reply bytes, and 32 events
per candidate. Requests and helper input/output also obey payload limits; kernel
endpoint and VM limits apply independently. Owner/candidate/pin/kernel counters
refuse before overflow. No identity reuse, eviction or implicit counter reset.
Host-issued code pins protect owner schemas, observations and pending work against
both unload APIs; last drop releases pins. Pending candidates prevent retirement.
Retiring and enrolling creates a fresh identity and leaves the tombstone behind.

## O1 correspondence and fixtures

| O1 definitions / witnesses | O2 concrete evidence |
| --- | --- |
| `RegionLayout`, `separateRegions`, `swap_region_frame` | `heterogeneous_storage_native_and_actual_wasm_candidates_are_independent`: separate inventory allocations, independent ledger and blob, metadata-only registry; actual native and guest candidates, unchanged live bytes. No swap refinement claimed. |
| `Domain.legal/propose`, `prepare_preserves_live`, `prepare_refusal`, `Witness.EntityFree` | Native inventory/ledger algorithms and blob `(state + observation) × request`; invariant/malformed/oversized/trap/fuel and native bound refusals preserve slots and candidate counters. |
| `preflight_observations`, `invalid_readonly_refused`, `Witness.QuerySafety` | Own query grant independently revoked/narrowed while dependency grant survives; dependency and transitive revocation, captured stamp/schema/caller corruption, retired/recreated identity and common gate refusal. |
| `wrong_invocation_refused`, `preparation_serial_advances` | Candidate validation binds caller, operation/phase/checkpoint/lease and current grants; detached capture refusal, finite staging/counter reservations and discard. |
| `prepare_pins_chain`, `pending_chain_unload_refused` | Owner/candidate/observation RAII pins; retirement refusal while pending; code unload refusal until last reference drops. Kernel retained-route fixture preserves captured helper chain while future dispatch changes. |
| Immutable guest preparation premise | New kernel `pure_preparation` fixtures: actual computation, native Read-import exclusion, guest nesting, shared fuel/calls/depth, detached bridge refusal, current/retained grants and executable routes. |

O3 must supply the sole sealed publication protocol: complete participant preflight,
visibility across swaps, reserved history/receipt bookkeeping, callback/allocation/
drop-free publication, replay/prefix and cleanup integration. O4 must accept dynamic
owner extensions, derived caches and live service/lifecycle integration. M4d must
accept codecs/restoration. Transfer/retirement publication, NetHack migration,
crash recovery and full ownership runtime acceptance remain unfinished.

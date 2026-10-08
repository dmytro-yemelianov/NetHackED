# nethacked-kernel (M3a)

M4-O2 adds `CallerView::call_pure(slot, request)` for authenticated native owner
ports. It invokes the native provider's declared Wasm helper with the **same**
fuel/call/depth budget and effective current plus retained grants/routes, imposing
a Read ceiling throughout the descendant computation. All descendant native
dispatches refuse before callbacks, including host ports labelled Read; declared
Wasm helpers/extensions remain callable. Guest input must contain its bounded own
snapshot and declared detached observations; this bridge supplies no live host
reads. Detached view clones have neither bridge nor new pin acquisition.

The returned `PureReply` includes detached bytes, exact executable resolution and
an RAII code pin for the helper's captured dependency closure. `pin_invocation`
retains caller/provider and retained invocation routes for owner query/candidate
staging; `Kernel::pin_modules` is host-only code ownership without authority.
Pins share the existing owner/reference bounds, use checked non-reused serials,
protect both unload APIs and release on last drop. Normal invocation remains
unchanged. The optional [`nethacked-owners`](../nethacked-owners/README.md) crate
owns independent storage and pure domain preparation; publication remains O3.

A generic, safe Rust byte runtime independent of the NetHack domain crates.
The optional M3b resource coordinator lives in the separate
[`nethacked-resources`](../nethacked-resources/README.md) crate. This kernel owns
no resource state or coherent M4 host persistence. Registration and invocation APIs are
trusted host APIs; guests
receive only the ABI below.

`Manifest` uses sorted export/point collections and ordered dependency slots.
Artifacts hash length-delimited code bytes, canonical sorted manifest/config
JSON, executable kind/ABI, and the attached target artifact when applicable.
For native registrations the host supplies the native artifact identity bytes;
this is not an inspection or hash of compiled Rust callback machine code.
Handles are private, kernel-instance-bound, monotonically generated values.
Reinstallation cannot inherit old handles or grants. Removing an extension
restores the original point implementation; the original handle/artifact stays
unchanged while an extension is attached.

Native callbacks own their data and receive only request bytes and an immutable
`CallerView`. Scope is a bounded set of exact owner/key pairs for grants. The
runtime intersects these sets and access ceilings at every call. Native ports
must decode their own schema and check the effective view before any effects;
opaque bytes cannot be universally checked for resource ownership. A port must
validate bounded input, output, accounting, and refusal before its own mutation.
There is no VM CPU guarantee for trusted native Rust code.

Only the host issues/revokes grants. Each slot binds the declared capability's
provider incarnation, endpoint, major, and bounded authority. Revoked grants,
stale providers, and mismatched access refuse before invoking that port. A read
endpoint itself imposes a read ceiling. Extensions inherit the requesting scope
and ceiling and use their own slot grants and authenticated executing identity.

`CallerView::permit(capability, resource, access)` inspects a bounded immutable
snapshot of the invoking module's live manifest/grants under this invocation's
effective ceiling. It includes the original provider and every resolved extension
handle/artifact/export, with no dispatcher or registry mutation. It uses no
coordinator grants and supplies no credentials for future calls. Stateful services
must obtain a fresh view and recheck permits on commit. `max_reply()` exposes the
actual native endpoint reply bound for serialization before effects.

Wasm ABI v1 exports `memory`, `alloc(i32) -> i32`, and declared callables
`(i32, i32) -> i64`. Results pack a pointer in the high 32 bits and length in
the low 32. The sole permitted import is
`kernel.call(slot, reqptr, reqlen, resptr, rescap) -> i32`; it returns a
nonnegative length or an `Error::abi_code()` refusal. Before dispatch, both
buffer ranges, request bound, and the endpoint's entire declared maximum reply
capacity are checked. Negative addresses/lengths and overflowing or out-of-range
buffers refuse. No WASI or ambient imports are provided. Starts, undeclared
exports, ABI conflicts, and invalid initial memory/table/data bounds refuse
before registration publication.

Guest memory is newly instantiated for every invocation, including nested
calls. Allocation and execution share fuel with all nested guest calls; fuel is
synchronized across suspended stores rather than replenished. Dispatch attempts
(including refused imports), payloads, module bytes, nesting, memory/table growth,
VM stack bytes/recursion, manifests, registration count, and grant scopes have
explicit limits. Memory/table/stack limits are per guest store; nesting bounds
the number of simultaneously resident stores. Exhausting call/depth limits
refuses additional dispatches; continued guest execution remains fuel bounded.

A call's successful effects are a committed prefix. A later refusal, invalid
output, or trap does not undo earlier successful native calls. Each native port
owns atomic single-call refusal. This is not an atomic pipeline, resource
transaction, prepared-plan service, or persistence guarantee.

The integration fixtures cover the populated entity-free host and computational
live extension, native/guest routing and artifact/handle validation, bounded
authority and committed prefixes, and adversarial loader/ABI/budget paths.
Acceptance requires package tests/clippy, formatting, and an actual standalone
`cargo build -p nethacked-kernel --target wasm32-unknown-unknown --locked`.
Independent semantic review and full workspace gates/goldens belong to the root
follow-up; this package alone does not complete M3 or A3.

## M4a retained execution and lifecycle

`capture_pipeline(provider, endpoint, ceiling)` publishes an opaque
`PipelineLease` only after a bounded traversal succeeds. It freezes the root
callable and every ordered extension resolution in the transitive declared
capability graph of every original/extension module encountered. This includes
native and guest code and permission-only Read slots to Write-capable resource
owner ports. All referenced registrations stay installed; the private immutable
routing map and registration artifacts bind execution to exact incarnations.
Declared cycles are unsupported and return `Cycle`; missing dependencies and
limits refuse before publication, reference changes, or serial advancement.
Capture is conservative: dependencies of intermediate originals are retained
also, even when an extension currently replaces their execution.

`invoke_pipeline(lease, request, authority)` calls only the captured root.
Authority must fit the captured root ceiling, otherwise it returns `Authority`.
Each nested call checks current caller registration, original provider and
major, live grant and its exact target, request/reply bounds, and ancestor
scope/access ceilings. It additionally intersects the live grant with that
slot's captured ceiling. A slot without an active issued grant at capture has
no retained authority, even if later granted; its code is still referenced.
Revoke/narrow affects the next resumed invocation immediately. Increasing a
grant cannot exceed its captured ceiling. Call, depth, fuel, memory and ABI
preflight follow the ordinary dispatcher; completed native effects remain a
committed prefix. Nothing captures guest RAM or native service state.

`CallerView::permit` uses the same immutable route map as retained execution,
with current grants and captured slot ceilings. Read participant permission
remains distinct from admission to a Write owner endpoint. Resource-service
prepare/commit can therefore validate the exact old chain after ordinary
dispatch changes, while still refusing revoked/narrowed participants. Separate
leases for prepare and commit demonstrate composition, not checkpoint ownership.

`pipeline_descriptor` exposes an immutable root, ceiling, ordered routes and
SHA-256 binding digest. The digest covers length-framed runtime-local handles,
artifacts, exports, route boundaries, and canonical root/slot authority data.
Native artifacts still identify host-supplied bytes, not callback machine code.
Lease and owner-reference identities have private instance/serial fields, no
wire constructors and no serde implementation. One checked monotonic serial
space is shared by leases and owner references; release never reuses identities.
Foreign, fabricated, released and stale identities return `InvalidHandle`.
These opaque runtime identities have no serialization or wire constructors.
The D3a registry-only seam below excludes live retained work; D3b captures the full kernel graph.

`RetentionLimits` bounds live leases, owner sets, total route binding entries,
captured slots, and aggregate references (one per unique module per lease/owner).
`set_retention_limits` requires no live reference owners. `retain_owner` takes a
bounded nonempty list of live handles, deduplicates it, and returns an opaque
`OwnerReference`; `release_owner` releases only that set. Optional resource,
schema and timer hosts must explicitly retain their own code references.
`release_pipeline` removes exactly its lease, leaving other owners untouched.
`lifecycle_state` provides counts and next counters for host diagnostics.

`managed_detach(extension)` removes its current dispatch attachment while
retained calls keep using the captured chain. `managed_attach(extension)` can
reattach only to its original live target incarnation and refuses occupied
points. `managed_unload(handle)` checks all retained/owner references, current
extension dispatch edges, and other registered modules' dependency/attachment
bindings before removal. Original dependency bindings and current capability
providers both protect targets, including after legacy unpinned replacement.
An unattached original with no dependents/references can be unloaded. Checks
refuse with `Referenced` before any registry, grant, dispatch or counter mutation.

Registration, grants, attachment, detach, pipeline capture, release and removal require
exclusive `&mut Kernel`; invocation holds `&Kernel` throughout all native and
Wasm calls. The kernel is not cloned and offers no interior-mutable lifecycle
or callback dispatcher. Callback captures and RAII code pins publish bounded weak
reference ownership through immutable invocation. Rust's exclusive borrowing enforces the safe boundary
throughout active calls, including when a host wraps the kernel in `RefCell`.
No guest-declared safe-point flag is accepted, and capture runs no guest/native
initialization. Low-level `unregister` preserves its tested unpinned removal
semantics (it may orphan dependents), but always checks retained/owner references.
It cannot bypass pins. New workflow/session hosts should use managed APIs.

Correspondence: `PipelineDescriptor` plus the private routing/code bindings
realize the executable-binding prerequisite of `Workflow.Pipeline`;
`PipelineLease`, managed lifecycle checks and both reference maps implement the
runtime counterpart of `Services.Lifecycle.BoundPipeline`/`liveReferences`.
These are concrete host interfaces, not a proved cross-language refinement.
M4b authoritative workflow frames/checkpoints/resource-operation ownership,
M4c input/timer callbacks and retry ownership, and M4d schemas/codecs/restoration
remain pending. No A4/A5 or NetHack migration acceptance follows from M4a.

## Operation context (optional workflow host)

`capture_operation` atomically admits a retained pipeline and a checked, non-reused
host operation identity. `operation_binding` and `invoke_operation` are trusted
host APIs, never guest-controlled payload APIs. The private immutable binding
includes operation namespace/serial, exact pipeline lease, checkpoint sequence and
phase. It propagates through all nested guest calls into `CallerView::operation`;
plain invocations expose `None`. `preflight_pipeline` performs root request and
authority admission before invocation. Resource/phase semantics and cleanup remain
in optional typed services; the kernel owns no workflow map or universal locator.
The workflow host must own lease lifetime and derive bindings from stored frames.
See [workflow trust and lifecycle](../nethacked-workflows/README.md).

Typed participants issue non-reused `CleanupIdentity` values without a service
registry. `operation_binding_with_cleanup` carries a bounded canonical identity
set from the workflow owner's frozen participant list. No guest constructor or
serialization exists. The plain binding API enrolls no participant.

## Authenticated native callback retention

An immutable `CallerView<'call>` offers only
`capture_callback(endpoint, ceiling) -> RetainedCallback`. Its private factory
borrows the kernel for the active call; that capture-capable view cannot
outlive the borrow. `view.clone()` preserves the existing owned metadata snapshot
API and returns a detached view without capture authority; snapshots may outlive
the call and refuse capture. The owning handle can outlive it. Native ports receive no
kernel, general dispatch function, or registry; the scoped guest-only pure bridge
is described above. Capture invokes no code and cannot adopt,
release leases, issue operations, change grants, or mutate executable dispatch.
Ports should validate their request schema before capture and publish ownership
only after their own validation. Guest JSON supplies an endpoint and requested
authority, never authenticated caller, artifact, host, or operation proof.

Capture requires an authenticated calling module, a declared scheduling import,
and an operation binding. Direct host/plain calls refuse. The endpoint must be
that calling module's public export (including when the caller is an executable
extension), with a bounded nonempty name. It needs no self-import. The separate
export's extension chain resolves during capture. Transitive imports already
resolved by the active operation retain that routing and intersect its slot
ceilings; additional routes resolve during capture. The callback root ceiling
must fit the actual native invocation. Absent historical grants remain absent,
even after regrant. Immutable provenance binds calling code, actual native
service code, declared capability/slot, and source operation/checkpoint/phase.

The scope policy requires the current live scheduling grant, bounded by its
declared access, to contain the **entire captured callback ceiling**, at both
adoption and every execution/preflight. Narrowing below that ceiling refuses
even if a particular execution requests less. Revocation refuses; regrant can
restore only authority within captured root/import ceilings. Nested imports
also apply ordinary live-grant checks. Source operation identity is provenance,
not the future execution identity or a requirement that the source lease live.

`adopt_callback(&handle)` is a trusted-host `&mut Kernel` safe boundary. It
validates host and live origin authority, then atomically issues a fresh
`(OperationId, PipelineLease)` over the retained routing without resolving current
dispatch. Ordinary binding, cleanup enrollment, preflight, invocation, resource,
and input mechanisms apply. `callback_provenance(lease)` exposes immutable origin
metadata on adopted leases. Dropping all capture handles does not invalidate
adopted leases; releasing one lease does not invalidate others. The host owns
each adopted lease and must call `release_pipeline`.

Handles clone shared `Rc` ownership of one capture. A weak index uses at most
`RetentionLimits::owners` entries, independently of explicit owner sets; dead
entries are pruned only on successful capture. Live captures also count against
aggregate `references`; routes, slots, depth, and names use existing bounds.
Captured routing and origin service chains protect all needed artifacts against
both removal APIs. Callback serials, operation IDs, and lease IDs use checked
monotonic counters. Refused admission changes no counters, references, or leases.
`lifecycle_state` reports live capture counts/references and the next callback
serial; cloning consumes no identity. Limits cannot change while references live.

Dropping the last unadopted handle immediately releases references, including
unpublished captures abandoned before native refusal or a later guest trap.
A capture successfully stored by a native port remains a committed prefix after
either fault. A successfully allocated but dropped capture consumes its serial;
this is not whole-call rollback. Internal capture index borrow conflicts refuse
with `NativeRefusal`. An outer `RefCell<Kernel>` still rejects reentrant mutation
during invocation; adoption must wait until the immutable borrow ends. There is
no ambient singleton, timer queue, once-only admission, delivery policy, codec,
or authoritative domain state in this primitive.

`preflight_callback(callback, request, authority, phase, participants)` performs
read-only admission on a captured callback using the same retained-route validation
as `preflight_pipeline`, plus future operation-context bounds. It consumes no
adoption counters or leases. Optional timer owners can therefore refuse invalid
requests before calling `adopt_callback`; native ports still receive only CallerView.


## D3a registry reconstruction

`registry_snapshot(&mut Kernel)` captures bounded local registration records,
exact native-v1/Wasm-ABI-v1 artifacts and manifests, ordered incarnation-bound
imports, original extension targets, current dispatch, active/revoked grants,
namespace history and generation/operation/empty-retention high-water counters.
Saved generation IDs are data. `restore_registry` builds a hidden kernel using
normal registration and the actual guest loader, compares the complete approved
manifest/kind and recomputed artifact (including the original target artifact),
then exchanges the destination only after complete admission. Multiple installed
detached extensions remain detached; no guest/native invocation or initialization
callback runs during reconstruction. Old opaque handles remain invalid.

`RegistryAuthority` must resolve exact allowlisted guest bytes or freshly created
native factories. Native factory purity, bounds, effect-free construction/drop,
and exclusion of source service aliases are explicit host assumptions. Saved
grants are requests: checked host resource bindings remap keys and independent
host policy approves destination authority. Approval may explicitly narrow access
or keys but cannot broaden the mapped request; revoked grants stay revoked.
Required unsupported authority must refuse. Current calls still check live grants.
Exact runtime limits must match the destination profile; retention limits may
narrow it. Namespace allocation excludes every saved historical/source namespace,
including in a fresh process, using bounded skips and checked exhaustion.

Live leases, owner references, pins, callbacks and pure captures refuse the
registry-only APIs. Use the D3b full-kernel seam for these values. Retired holes and
empty-retention serial history survive. These records are not a competing wire
transport: D4 will adapt this seam to D1's bounded envelope and the complete host
barrier/authority exclusion. D3c owner prepared/retained state,
D4–D6/A5 and M5–M7 remain open; aggregate D3 is not reduced by D3a.


## D3b complete retained-kernel reconstruction

`kernel_snapshot(&KernelEnrollment)` records every live lease, owner reference,
shared pin and shared callback beside private retained state. The exclusive host
explicitly enrolls externally held `OperationBinding` and `PureCapture` values;
the kernel cannot discover their host storage. Enrollment does not replace or
omit the automatically captured kernel graph. Operation phases/checkpoints and
canonical cleanup sets survive. Serial highwaters and holes remain nonreusable.
No owner payload, workflow frame, guest RAM or service state is stored here.

`restore_kernel` reuses D3a artifact factories, registry/grant/resource admission
and namespace exclusion. It validates bounded records and complete exact routes,
ordered import slots, artifacts, signatures, crosslinks, distinct shared IDs,
source digests and historical source tombstones before one hidden exchange.
Destination grants must cover every retained slot ceiling, including historical
source closures and pure captures; revocation or narrowing needed authority
refuses rather than changing retained semantics. Routes and fresh binding digests
are reconstructed independently of current dispatch. Current and resumed calls
continue checking live grants.

Callback origins retain the full caller/service/artifact/capability/slot/source
binding and an exact source-closure witness even after source lease disposal.
Adoption uses the retained executable closure. Pure captures carry a private
route witness and pin their provider plus complete helper closure. Shared pins
and callbacks have one reservation each; clones share lifetime and block unload
until their final release. No invocation, callback, preparation, input draw or
scheduling runs during capture/rebuild.

`RetainedAuthority` additionally approves complete operation contexts through
`approve_operation_context`; the default refuses. Supply a trusted, independent
host catalog or policy before reading an incoming save. Approval covers enrolled
operations and historical/disposed callback sources: namespace, operation,
lease, checkpoint, phase, cleanup, caller/service artifacts, capability/slot,
and exact source and retained route identities. Saved digests, ranges and two
matching wire copies are not authentication. The policy must be bounded, pure
and stable; D4 later composes this seam with service admission. A bounded metadata
pass validates graph cross-references, closures and recomputed unique reservation
references before artifact factories. Callbacks have their own capacity, separate
from the combined owner/pin capacity; shared handles count once per reservation.

`RetainedAuthority` also approves cleanup participants. Runtime-issued
fresh cleanup identities exclude every saved historical origin, also on a fresh
process. `RestoredKernel` exposes only sealed fresh mappings for later D3c/D4
adapters, including nonexecutable historical source IDs. Its RAII maps own one
reference: adapters clone the required pins/callbacks/captures, then drop the maps
to release their reference. Old destination/source credentials remain invalid
following successful exchange; refusal preserves the populated destination and
external credentials. Native factory purity, bounds, effect-free construction
and drop, absence of source aliases, and stable host admission remain explicit
host assumptions.

These are bounded local graph records, not a second wire transport. D4 supplies
the D1 envelope and complete host barrier/authority exclusion; the exclusive
kernel boundary excludes active calls. D3c owner candidates, aggregate D3,
D4 services, D5–D6 and A5 remain successors and are not accepted by this seam.

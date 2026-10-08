# nethacked-kernel (M3a)

A generic, safe Rust byte runtime independent of the NetHack domain crates.
The optional M3b resource coordinator lives in the separate
[`nethacked-resources`](../nethacked-resources/README.md) crate. This kernel owns
no resource state or M4 persistence. Registration and invocation APIs are
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
There is no restart identity, schema version, snapshot or restoration codec.

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

Registration, grants, attachment, detach, capture, release and removal require
exclusive `&mut Kernel`; invocation holds `&Kernel` throughout all native and
Wasm calls. The kernel is not cloned and offers no interior-mutable lifecycle
or callback dispatcher. Rust's exclusive borrowing enforces the safe boundary
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

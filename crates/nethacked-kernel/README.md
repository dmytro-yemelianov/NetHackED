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

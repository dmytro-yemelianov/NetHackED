Compatible live Wasm and detached queries (O4a)
=============================================

`Backend::Wasm` retains its enrollment-pinned command behavior and native schema
query until the trusted host explicitly calls `admit_compatible_wasm`. Admission
requires the same schema identity, codec encoding, dependency capability/slot,
and an exact current Wasm ABI/artifact chain. It preserves populated state and
provider-bound owner authorization. It grants no authority. Trusted codecs,
native purity, artifact fidelity and absence of mutable aliases remain assumptions.
Incompatible schemas require migration, which this API does not implement.

Admission changes future commands and query computations. Already prepared
commands retain their original codec, executable captures and proposals;
publication uses their original operation routing and freshly checked grants.
New current dispatches must resolve the admitted chain. Retained old dispatches
cannot silently select a new chain. Explicit compatible admission after detach
can select the original chain again. Checked behavior counters never wrap.

A query observation privately retains its actual code, own Read permit and exact
ordered transitive observations. Helper capture validation is nonexecuting,
lifetime-bound, uses current or retained routing as appropriate, and compares the
complete conservative declared helper closure and grant ceilings. Detached views
cannot acquire captures or validate them. Native helper descendants refuse.

`query_cache` returns one shared, disposable, single-entry cache handle per owner;
the owner keeps only a weak handle. Retaining results can keep owner/code alive,
but cannot form an owner/cache cycle. Dropping handles releases references.
`query_cached` validates or recomputes under one visibility gate. It binds the
query name/request, own stamp/Read authority, behavior revision, ordered transitive
observations, helper captures and host-configured profile/input-policy identity.
Hits skip codecs and algorithms. Changes to supplied dependencies recompute;
invalid supplied credentials or stale observations refuse. Cache/code bounds are
owner limits plus kernel reference/closure limits, with no eviction of identities.

`QueryPolicy::Deterministic` is a trusted host purity assertion; requests and guest
outputs cannot select credentials, profiles or recorded context. External or
varying inputs must use `Noncacheable` (ordinary uncached `query`); authenticated
recorded-context composition is outside this API. Policy/admission changes clear
the disposable entry. Cache validation and lifecycle operations refuse Busy during
publication. This unit does not establish aggregate O4 acceptance or implement
input, timer, workflow or full entity-free composition.

Review repairs F1/F2
-------------------

Current query/cache validation and fresh preparation/readonly batch admission
require the current behavior revision recursively. Sealed candidates and batches
instead revalidate their captured observations under their exact issued operation,
pipeline lease, checkpoint and phase. This internal distinction allows a future
compatible behavior or policy admission while preserving the captured proposal and
codec. It does not relax owner stamps, schema, live grants, caller identity, retained
helper routes/code or visibility gating. A current invocation cannot choose retained
validation through guest input.

Focused repair fixtures prepare an owner from a derived observation (also through
a recursive observation), change the dependency's future algorithm/policy, and
commit the original proposal with its readonly dependencies. Stale state and
revoked authority still refuse; current observations and new admissions refuse the
old behavior, and a fresh cache computes the new result. Additional WAT fixtures
import `kernel.call` and execute an independently registered guest descendant,
including a declared-dependency extension wrapper. Computed output markers and
codec/native counters prove positive execution, nonexecuting validation refusal
under finite descendant grant revocation/narrowing, current versus retained code
routing, and native descendant refusal before its callback. These checks retain
owner state/history and the existing native purity, codec and alias assumptions.

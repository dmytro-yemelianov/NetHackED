# M4c-timer-capture: bounded authenticated callback retention

Prerequisite for the optional timer queue: a native scheduling port is invoked with immutable `CallerView`, not a mutable Kernel. It must be able to retain an authorized callback while the invoking module and its resolved code are still authenticated. A delayed host recapture using current dispatch would incorrectly select newly attached code. Implement this generic kernel mechanism before the timer queue; do not add timer policy or NetHack state here.

## Required mechanism

Provide an opaque host-bound retained callback/continuation handle accessible through a bounded method on the current `CallerView`. Guests request a bounded callback endpoint name through their scheduling port; the trusted port obtains the handle from that view. The minimum supported target is an exported endpoint of the authenticated calling module itself, with its currently resolved executable extension chain. Owning a module's public export avoids a self-dependency cycle merely to retain its own callback. Additional target forms are unnecessary here. A guest cannot supply another caller, host, artifact, pipeline, authority or operation as proof. Missing/unbound caller/operation or absent export refuses. Native callbacks still receive no Kernel, World, universal context, arbitrary dispatch function or mutable registry.

Capture the actual closed executable routing for the selected endpoint and its transitive imports at that moment, including code identities and authority ceilings. Bind the creating caller/artifact, creating native service/provider/artifact and its declared permission path, source operation/checkpoint provenance, and host namespace. The callback is a separate future invocation, not a replay of the source phase. Its root authority cannot exceed the creating invocation; imported calls must obey both captured ceilings and current live grants. Current authority on the creating caller's scheduling capability must be revalidated before host adoption/execution; revocation or insufficient current scope cannot be bypassed by a captured handle. Choose an explicit documented scope policy and test it; no historical permission snapshot can replace live checks. Record immutable creation provenance without granting authority to guest JSON.

The handle retains every executable dependency needed by callback execution and origin validation. Every removal API must refuse while such a live handle or adopted pipeline references its artifact. New extension attachment changes future captures; existing callbacks keep old resolution. Cloning host handles shares one retained capture; abandoning/dropping the last unadopted handle releases its references without mutable kernel access, including when the native port/guest later refuses or traps. No leaked reference from an unpublished capture. Bookkeeping remains finitely bounded; weak indexes or an equivalent owned design are acceptable. Retention/identity counters never wrap or reuse, and limit/refusal changes no semantic metadata. Trusted native side effects remain committed prefixes; do not claim whole-phase rollback.

Expose a narrow trusted-host adoption operation at a safe boundary: validate the handle against this Kernel and current authority, then acquire a fresh operation ID plus ordinary pipeline lease over the captured routing (without resolving current dispatch again). This connects to existing workflow/timer ownership. After adoption, releasing all capture handles must not invalidate the adopted lease; releasing one adopted lease must not invalidate another. Failed adoption changes no identities, live references or leases. Existing invocation/preflight/resource/input mechanisms must work with an adopted operation exactly as with other retained operations. The timer owner will enforce once-only admission and callback failure semantics in the next unit; this primitive alone does not claim once-only delivery.

Use the smallest internal interface change needed. Borrowed capability methods or private typed factories are allowed; they must not expose general kernel access. Preserve existing public call paths and existing tests/helpers/assertions. Do not redesign manifests, add mandatory timer/clock/entity fields or copy authoritative domain state. Explain the exact native lifetime/ownership and reentrancy rules in the kernel README and contract. Do not require a global ambient singleton/service locator.

## Required evidence

1. Actual guest -> trusted native scheduling port captures that guest module's separate callback export while an authoritative operation is executing. The source operation is then retired; adopted callback still executes its retained guest code and nested native capability on a populated entity-free host. Same-module public callback export works without a self-import dependency cycle.
2. Attach changed executable behavior after capture: old captured/adopted callback retains old behavior, new capture gets new behavior. Verify every removal path, nested dependency retention, shared handles, multiple adoptions, safe release and unrelated state preservation. Adoption must demonstrably avoid current-dispatch recapture.
3. Negative controls: plain invocation, wrong host, missing callback export, forged request identity, broader requested authority, revoked/narrowed creating grant and nested live grant, capture/adoption capacity/counter overflow. Keep valid controls and exact semantic metadata/state comparisons; regrant only restores authority allowed by the original ceiling, never newly broadens it.
4. Capture dropped before native refusal or later guest trap releases its unpublished references; a successfully stored capture survives the source fault as a committed prefix. Reentrancy/borrow refusal is safe, and clone/drop/adoption ownership has no leak or premature unload. Exhaustion tests can use private test-only counter setup.
5. Adopted operation carries its new authenticated operation identity through nested calls; source operation/checkpoint is immutable provenance, not the new execution identity. Compose at least one existing resource or input port to verify the accepted context/authority machinery remains effective.

Allowed edits: kernel src/README, new kernel tests/callbacks.rs (and a new callbacks support module if needed), NEW resource or input integration test file if required, and this contract. No existing tests/helpers, Cargo/dependencies, workflow/resource/input production, domain logic or goldens. If an actual necessary boundary exceeds scope, return concrete evidence instead of silently broadening. Focused kernel plus affected new integration tests/fmt/clippy and actual kernel wasm32 build precede fresh semantic review and root full12/all3. Preserve raw commands/exits/source hashes/failure history. Timer queue/callback policy, durable codec, vanilla adapter and domain migration remain explicit successors.

## Implementation policy and evidence

`CallerView<'call>::capture_callback` is a borrowed private capture-only factory,
not kernel access. It retains a public export of the actual authenticated caller,
including a calling executable extension. A separate callback root resolves at
capture time; imports already present in the active operation retain its code
resolution and intersect its captured ceilings. Additional transitive routes
resolve during capture. The source operation need not remain live afterward.

The requested callback ceiling must fit the native invocation. The creating
scheduling grant must currently contain the **full captured callback ceiling**
under declared access, at adoption and at every adopted pipeline preflight or
execution. Narrowing below it refuses even when the execution requests less.
Ordinary nested live checks intersect historical ceilings; regrant cannot add
permission absent from the captured closure. Guest request identity fields are
not accepted as authentication.

Opaque `RetainedCallback` clones share one allocation. A bounded weak index
(`owners` entries, separately from explicit owner sets) tracks live captures;
dead entries are pruned only on successful publication. Captures count against
aggregate references and use existing route/slot/depth/name limits. Last-handle
drop releases unpublished references without kernel mutation. A successfully
allocated serial remains consumed after drop or a subsequent fault. Successfully
stored native captures remain committed prefixes. Borrow conflicts refuse safely.

Trusted host `adopt_callback` atomically issues a new operation and ordinary
lease over captured routing, without current-dispatch recapture. Each lease
independently owns callback and origin-validation artifacts. Creation provenance
remains accessible from the handle and adopted lease; it never replaces the new
execution identity. Native invocation borrows the kernel immutably, so reentrant
lifecycle mutation and adoption must wait for that borrow to end. The original
capture-capable view cannot escape its borrow; the owned handle can. Existing
`view.clone()` calls return detached owned metadata snapshots, preserving their
storage API while refusing later capture. No self-import is needed.

Evidence is in new `nethacked-kernel/tests/callbacks.rs`, shared
`callbacks_support/mod.rs`, private `src/callbacks_tests.rs` counter controls,
and new `nethacked-resources/tests/callbacks.rs`:

1. Real guest scheduling and same-module callback exports, retired source,
   populated entity-free state, nested native effect, and fresh nested identity.
2. Old/new executable callback extensions and nested dispatch replacements,
   actual extension caller authentication, both removal APIs, clones/multiple
   adoptions, independent release, and unrelated state preservation.
3. Plain/unbound calls, foreign hosts, absent export, forged JSON, authority
   widening, revoked/narrowed creating and nested grants, absent historical
   grants, node/slot/reference/capture/lease bounds, and both identity overflows.
4. Unpublished capture drop before refusal/trap, stored committed prefixes,
   storage/index/outer-kernel borrow refusal, weak-index reuse, and exact counts.
5. Fresh operation propagation and resource prepare/commit with cleanup
   enrollment, owner permission, exact operation/checkpoint/phase matching,
   live scheduling revocation, and exact final resource state.

Raw focused commands, exits, per-command source hashes, and unreplaced failure
history are retained at `/tmp/m4c-callback-evidence`. Independent semantic review,
root full12/all3 verification, and commit remain subsequent root-owned work;
this unit adds no timer queue or delivery semantics.

## Workspace compatibility repair

The first full-workspace Clippy gate found that the existing resources
`boundary_tests::fixture` return type needs explicit `CallerView<'static>`
annotations after the view acquired its borrowed capture capability. Its stored
views already come from the reviewed detached `clone()` path. The coordinator
permits this exact test-setup-only annotation under the global constraint allowing
setup changes: no helper body, assertion or production resource code changes.
This is compatibility repair 3, retaining the writer's two prior clone/lint
repairs in the same history. Independent narrow readback and fresh full gates
are required; the earlier focused allocation did not cover this library test.

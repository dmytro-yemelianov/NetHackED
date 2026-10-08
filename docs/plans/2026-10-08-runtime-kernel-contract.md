# M3: runtime kernel and executable modules

Implement the [architecture goal](2026-10-08-microkernel-goal.md) against the reviewed Lean contracts. This unit proves generic A1/A2/A3 scenarios. M4 supplies durable workflows/codecs/lifecycle services; M5 supplies the populated NetHack host and full selected-wand slice. A standalone runtime does not close the goal or parity backlog.

## Package and backend

Create `nethacked-kernel`, independent of every NetHack domain crate. Retain registration, versioned capabilities, authenticated invocation identities, dispatch and executable binding in the kernel. Host/domain state belongs to registered native modules or bounded payload adapters. Resource transactions are an optional service, not a compulsory world or entity schema.

Use pinned `wasmi = 2.0.0` with default features disabled and `std`, `stable`, `validate`, `portable-dispatch`, `prefer-btree-collections`; verify actual wasm32 compilation. [Upstream](https://github.com/wasmi-labs/wasmi) supplies the interpreter, fuel metering and store limits. The installed crate declares Rust 1.86; the repository uses 1.88. Use existing serde/serde_json/sha2 versions where practical; a compatible `wat` dependency is test-only. Update Cargo.lock once, then use locked checks.

## Public boundaries

- Module manifests declare unique IDs, versioned callable exports, ordered required capabilities and explicit extension points. Exports use opaque byte payloads, allowing different host schemas and algorithms. No fixed gameplay command enum or world/hero/item/clock/RNG field.
- Native ports own their data. Their callback receives only input bytes and an authenticated, immutable caller/grant view. No public mutable Kernel or universal host context is passed to a domain handler.
- Loaded Wasm bytes plus canonical manifest/configuration are hashed and bound to that registration. Guest callers cannot provide their own module, host, artifact or grants. Public module handles have private construction, kernel-instance binding and registration generation; cross-kernel and stale handles refuse.
- A capability is identified by provider, endpoint and major version. Host-issued grants must match declared dependencies and include bounded resource permissions when applicable. Calls resolve ordered slots from the loaded manifest. Missing, wrong-version, revoked and out-of-scope calls refuse before their own effects.
- Guest-provided callable endpoints must be available through declared capability routing, as well as trusted host invocation. Native and guest provider code runs under its own authenticated binding. Bound nested calls need depth, call-count and aggregate execution budgets; no ambient authority or WASI imports.
- Read-only invocations cannot acquire write authority through a provider's broader grants. Propagate an effective access ceiling and intersect resource permissions through nested calls; native ports receive that immutable view. A provider cannot expand the requesting call's delegated scope.
- Extensions name an installed module and exported point, register their own executable artifact and grants, and alter that point's algorithm. Deterministic ordering and duplicate/dependency/point/signature checks occur before publishing registration. The original host stays populated. New commands may be added through the new module's exports. Preserve callable identities needed for M4 pending-pipeline retention.

## Wasm ABI v1

Use exported `memory`, bounded `alloc(i32) -> i32` and declared callable exports `(i32, i32) -> i64`. Arguments are an opaque input buffer. A result packs pointer in the high 32 bits and byte length in the low 32 bits. Validate every range with checked arithmetic and maximum payload lengths.

The only permitted host import is `kernel.call(slot, request_ptr, request_len, response_ptr, response_capacity) -> i32`; nonnegative return is response length, negative return is refusal. Slots bind to declared versioned capabilities and live host grants. Preflight the response buffer against the endpoint's declared maximum reply before invoking a mutating port. Invalid pointers/lengths/capacities must cause no call effects. Inspect the actual dependency API rather than guessing methods.

Reject start functions and incompatible exports/imports during admission. Fuel covers allocation, execution and nested guest calls; limits cover module bytes, memory/tables/stack, payloads, host calls and nesting. Validate output pointers/lengths after execution. A trap cannot erase an already committed legitimate prefix; document that boundary and test a commit followed by refusal/trap. Native ports are trusted code and must have their own bounded validation, not a claimed VM CPU guarantee.

## Optional resource service

Provide owner/key scoped opaque byte resources with generations, revisions and limits. A trusted host initializes/inspects resources without giving guests administration. Generation reuse after whole deletion creates a different incarnation; all bounded counters use checked arithmetic.

Expose read, prepare and commit ports through the same grant checks. A prepare request declares readonly dependencies, unique bounded writes and ordered events. Validate scopes, current stamps, payload bounds and supplied owner/accounting validators before allocation. Store exact immutable plans under host-bound identities and actual caller/artifact bindings. The guest commits only the identity, never substituted writes. Commit rechecks current permissions/code, all readonly/write stamps and validators; success updates all participants/history together and consumes the plan once. Refusal preserves resources, pending plan, history and counters. No mandatory item/actor types or global clock.

## Required runtime fixtures

1. An entity-free populated device/scalar host uses bounded payloads and preserves an unrelated field. Load actual Wasm bytes, invoke a computational algorithm, attach a second executable extension to the existing instance, then observe changed behavior and an added command without rebuilding the engine or reconstructing the host.
2. Exercise native and guest capability providers, module/extension dependency and point validation, canonical artifact binding and module-handle host/generation isolation.
3. Exercise unauthorized read/write, wrong major/slot/scope, revocation and another artifact attempting to commit a prepared plan. Compare exact service state before/after refused operations.
4. Exercise multi-owner successful mutation/history, stale readonly/write revisions, generation reuse, duplicate participants, invalid owner/accounting preflight and duplicate commit. Keep unrelated resources unchanged.
5. Exercise forbidden imports/start, malformed/signature-invalid modules, fuel exhaustion, excessive memory growth, bad buffers/oversized output and insufficient reply capacity before a mutating host call. Verify aggregate nested-call budgets and bounded refusal paths.

Use affected package tests/clippy/fmt and an explicit `cargo build -p nethacked-kernel --target wasm32-unknown-unknown --locked`. Existing full WASM gate currently covers a different package; it cannot substitute for that check. Run fresh independent semantic review and all twelve gates/three unchanged goldens before committing stable code. Existing tests/helpers/golden constants remain unchanged. Keep failed attempts and exact final inputs bound in external evidence.

## Execution split

M3a implements the kernel/ABI/loader, native and guest callable providers, bounded grants and actual live extension fixtures. Use small native test ports for authority/refusal and committed-prefix checks. M3b then implements the optional resource coordinator, its scoped stamped atomic transactions and complete A3 fixtures through those accepted ports. The split follows separate owners; neither child alone completes M3. M4 remains pending and no NetHack source migration occurs in either child.

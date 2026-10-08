use nethacked_kernel::{
    Access, Authority, Capability, Dependency, Endpoint, Error, Extension, Kernel,
    Limits as KernelLimits, Manifest, ModuleHandle, NativePort, Resource, Scope,
};
use nethacked_resources::*;
use serde::{Deserialize, Serialize};
use std::{
    cell::Cell,
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};

const WIRE: u32 = 8192;
fn address(owner: &str) -> Resource {
    Resource {
        owner: owner.into(),
        key: "value".into(),
    }
}
fn cap(owner: &str) -> Capability {
    Capability {
        provider: owner.into(),
        endpoint: "guard".into(),
        major: 1,
    }
}
fn ep(access: Access) -> Endpoint {
    Endpoint {
        major: 1,
        access,
        max_request: WIRE,
        max_reply: WIRE,
    }
}
fn manifest(id: &str) -> Manifest {
    Manifest {
        id: id.into(),
        exports: BTreeMap::new(),
        dependencies: vec![],
        extension_points: BTreeSet::new(),
        extension: None,
    }
}
fn provider(k: &mut Kernel, owner: &str) -> ModuleHandle {
    provider_code(k, owner, owner.as_bytes())
}
fn provider_code(k: &mut Kernel, owner: &str, artifact_bytes: &[u8]) -> ModuleHandle {
    let mut m = manifest(owner);
    m.exports.insert("guard".into(), ep(Access::Write));
    m.extension_points.insert("guard".into());
    let port: NativePort = Rc::new(|_, _| Ok(vec![]));
    k.register_native(m, artifact_bytes, BTreeMap::from([("guard".into(), port)]))
        .unwrap()
}
fn authority(access: Access, owners: &[&str]) -> Authority {
    Authority {
        access,
        scope: Scope::Keys(owners.iter().map(|s| address(s)).collect()),
    }
}
fn guest(k: &mut Kernel, id: &str, owner_names: &[&str], readonly: &[&str]) -> ModuleHandle {
    guest_code(k, id, owner_names, readonly, "")
}
fn guest_code(
    k: &mut Kernel,
    id: &str,
    owner_names: &[&str],
    readonly: &[&str],
    marker: &str,
) -> ModuleHandle {
    let mut m = manifest(id);
    for (endpoint, access) in [
        ("read", Access::Read),
        ("prepare", Access::Write),
        ("commit", Access::Write),
    ] {
        m.exports.insert(endpoint.into(), ep(access));
        m.dependencies.push(Dependency {
            capability: Capability {
                provider: "resources".into(),
                endpoint: endpoint.into(),
                major: 1,
            },
            access,
        });
    }
    m.exports.insert("small".into(), ep(Access::Write));
    m.dependencies
        .extend(owner_names.iter().map(|owner| Dependency {
            capability: cap(owner),
            access: if readonly.contains(owner) {
                Access::Read
            } else {
                Access::Write
            },
        }));
    let mut wrappers = String::new();
    for (name, slot, capacity) in [
        ("read", 0, WIRE),
        ("prepare", 1, WIRE),
        ("commit", 2, WIRE),
        ("small", 1, 1),
    ] {
        wrappers.push_str(&format!(
            r#"(func (export "{name}") (param i32 i32) (result i64)
            i32.const {slot} local.get 0 local.get 1 i32.const {capacity} call $forward)"#
        ));
    }
    let bytes = wat::parse_str(format!(r#"(module
        (import "kernel" "call" (func $call (param i32 i32 i32 i32 i32) (result i32)))
        (memory (export "memory") 1)
        (func (export "alloc") (param i32) (result i32) i32.const 0)
        (func $forward (param $slot i32) (param $ptr i32) (param $len i32) (param $capacity i32) (result i64)
            (local $n i32)
            local.get $slot local.get $ptr local.get $len i32.const 32768 local.get $capacity call $call local.set $n
            local.get $n i32.const 0 i32.lt_s
            if i32.const 32768 local.get $n i32.store i32.const 4 local.set $n end
            i64.const 140737488355328 local.get $n i64.extend_i32_u i64.or)
        {marker} {wrappers})"#)).unwrap();
    k.register_guest(m, &bytes).unwrap()
}
fn call_raw(
    k: &Kernel,
    caller: ModuleHandle,
    endpoint: &str,
    bytes: &[u8],
    auth: Authority,
) -> std::result::Result<Vec<u8>, i32> {
    let reply = k
        .invoke(caller, endpoint, bytes, auth)
        .map_err(Error::abi_code)?;
    if reply.len() == 4 {
        let code = i32::from_le_bytes(reply.clone().try_into().unwrap());
        if code < 0 {
            return Err(code);
        }
    }
    Ok(reply)
}
fn call<T: Serialize, R: for<'de> Deserialize<'de>>(
    k: &Kernel,
    caller: ModuleHandle,
    endpoint: &str,
    input: &T,
    auth: Authority,
) -> std::result::Result<R, i32> {
    let bytes = call_raw(
        k,
        caller,
        endpoint,
        &serde_json::to_vec(input).unwrap(),
        auth,
    )?;
    Ok(serde_json::from_slice(&bytes).unwrap())
}
struct Fixture {
    k: Kernel,
    service: Service,
    caller: ModuleHandle,
    service_handle: ModuleHandle,
    providers: Vec<ModuleHandle>,
    stamps: Vec<Stamp>,
    owners: Vec<&'static str>,
    owner_valid: Rc<Cell<bool>>,
    accounting_valid: Rc<Cell<bool>>,
}
impl Fixture {
    fn with_limits(limits: Limits) -> Self {
        let owners = vec!["item", "equip", "timer", "debt", "rules", "unrelated"];
        let mut k = Kernel::new(KernelLimits::default()).unwrap();
        let accounting_valid = Rc::new(Cell::new(true));
        let flag = accounting_valid.clone();
        let service = Service::new(limits, Rc::new(move |_| flag.get())).unwrap();
        let service_handle = service.install(&mut k, "resources").unwrap();
        let owner_valid = Rc::new(Cell::new(true));
        let mut providers = vec![];
        let mut stamps = vec![];
        for owner in &owners {
            let h = provider(&mut k, owner);
            providers.push(h);
            let flag = owner_valid.clone();
            let expected_owner = *owner;
            service
                .register_owner(
                    owner,
                    OwnerPolicy {
                        capability: cap(owner),
                        provider: h,
                        artifact: k.artifact(h).unwrap(),
                        validate: Rc::new(move |declared| {
                            assert!(declared
                                .snapshots
                                .iter()
                                .all(|s| s.stamp.address.owner == expected_owner));
                            assert!(declared
                                .plan
                                .writes
                                .iter()
                                .all(|w| w.stamp.address.owner == expected_owner));
                            flag.get()
                        }),
                    },
                )
                .unwrap();
            stamps.push(
                service
                    .put(address(owner), owner.as_bytes().to_vec())
                    .unwrap(),
            );
        }
        let caller = guest(&mut k, "caller", &owners, &["rules"]);
        let mut f = Self {
            k,
            service,
            caller,
            service_handle,
            providers,
            stamps,
            owners,
            owner_valid,
            accounting_valid,
        };
        f.grants(caller);
        f
    }
    fn new() -> Self {
        Self::with_limits(Limits {
            wire: WIRE,
            ..Limits::default()
        })
    }
    fn grants(&mut self, caller: ModuleHandle) {
        for slot in 0..3 {
            self.k
                .issue_grant(
                    caller,
                    slot,
                    authority(
                        if slot == 0 {
                            Access::Read
                        } else {
                            Access::Write
                        },
                        &self.owners,
                    ),
                )
                .unwrap();
        }
        for (index, owner) in self.owners.iter().enumerate() {
            self.k
                .issue_grant(
                    caller,
                    index + 3,
                    authority(
                        if *owner == "rules" {
                            Access::Read
                        } else {
                            Access::Write
                        },
                        &[*owner],
                    ),
                )
                .unwrap();
        }
    }
    fn plan(&self) -> Plan {
        Plan {
            readonly: vec![self.stamps[4].clone()],
            writes: vec![Write {
                stamp: self.stamps[0].clone(),
                replacement: None,
            }],
            events: vec![b"deleted".to_vec()],
        }
    }
    fn prepare(&self, plan: &Plan) -> Identity {
        call(
            &self.k,
            self.caller,
            "prepare",
            plan,
            authority(Access::Write, &self.owners),
        )
        .unwrap()
    }
    fn commit(&self, id: &Identity) -> std::result::Result<Identity, i32> {
        call(
            &self.k,
            self.caller,
            "commit",
            id,
            authority(Access::Write, &self.owners),
        )
    }
    fn refuses<T: Serialize>(&self, endpoint: &str, value: &T) {
        let before = self.service.state().unwrap();
        assert!(call::<_, serde_json::Value>(
            &self.k,
            self.caller,
            endpoint,
            value,
            authority(Access::Write, &self.owners)
        )
        .is_err());
        assert_eq!(self.service.state().unwrap(), before);
    }
}
#[test]
fn real_guest_read_prepare_commit_readonly_and_replay() {
    let f = Fixture::new();
    let snapshot: Snapshot = call(
        &f.k,
        f.caller,
        "read",
        &address("item"),
        authority(Access::Write, &f.owners),
    )
    .unwrap();
    assert_eq!(snapshot.stamp, f.stamps[0]);
    let before = f.service.state().unwrap();
    let id = f.prepare(&f.plan());
    let pending = f.service.state().unwrap();
    assert_eq!(pending.resources, before.resources);
    assert_eq!(pending.history, before.history);
    assert_eq!(pending.commits, before.commits);
    // Both refusals target a live plan; a missing pending ID cannot mask them.
    f.refuses(
        "commit",
        &Identity {
            service: id.service + 1,
            serial: id.serial,
        },
    );
    f.refuses(
        "commit",
        &serde_json::json!({"service":id.service,"serial":id.serial,"writes":[],"events":[]}),
    );
    assert_eq!(f.service.state().unwrap(), pending);
    assert_eq!(f.commit(&id), Ok(id.clone()));
    let after = f.service.state().unwrap();
    assert!(!after.resources.contains_key(&address("item")));
    assert_eq!(
        after.resources[&address("rules")],
        before.resources[&address("rules")]
    );
    assert_eq!(after.history, vec![b"deleted".to_vec()]);
    f.refuses("commit", &id);
    f.refuses(
        "commit",
        &Identity {
            service: id.service,
            serial: id.serial + 10,
        },
    );
    f.refuses(
        "prepare",
        &serde_json::json!({"caller":"forged", "readonly":[], "writes":[], "events":[]}),
    );
}
#[test]
fn changed_permissions_stamps_and_policies_refuse_with_exact_pending_state() {
    for defect in [
        "revoke-read",
        "revoke-write",
        "narrow-read",
        "narrow-write",
        "read-access",
        "readonly-stale",
        "write-stale",
        "recreate",
        "owner",
        "accounting",
        "resolution",
        "provider",
    ] {
        let mut f = Fixture::new();
        let id = f.prepare(&f.plan());
        match defect {
            "revoke-read" => f.k.revoke_grant(f.caller, 7).unwrap(),
            "revoke-write" => f.k.revoke_grant(f.caller, 3).unwrap(),
            "narrow-read" => {
                f.k.issue_grant(f.caller, 7, authority(Access::Read, &[]))
                    .unwrap()
            }
            "narrow-write" => {
                f.k.issue_grant(f.caller, 3, authority(Access::Write, &[]))
                    .unwrap()
            }
            "read-access" => {
                f.k.issue_grant(f.caller, 3, authority(Access::Read, &["item"]))
                    .unwrap()
            }
            "readonly-stale" => {
                f.service.put(address("rules"), b"new".to_vec()).unwrap();
            }
            "write-stale" => {
                f.service.put(address("item"), b"new".to_vec()).unwrap();
            }
            "recreate" => {
                f.service.delete(&address("item")).unwrap();
                let stamp = f.service.put(address("item"), b"item".to_vec()).unwrap();
                assert_ne!(stamp.generation, f.stamps[0].generation);
            }
            "owner" => f.owner_valid.set(false),
            "accounting" => f.accounting_valid.set(false),
            "resolution" => {
                let mut m = manifest("extension");
                m.exports.insert("guard".into(), ep(Access::Write));
                m.extension = Some(Extension {
                    target: cap("item"),
                    export: "guard".into(),
                });
                let port: NativePort = Rc::new(|_, _| Ok(vec![]));
                f.k.register_native(m, b"new-code", BTreeMap::from([("guard".into(), port)]))
                    .unwrap();
            }
            "provider" => {
                let old_artifact = f.k.artifact(f.providers[0]).unwrap();
                f.k.unregister(f.providers[0]).unwrap();
                let new_provider = provider_code(&mut f.k, "item", b"item-code-v2");
                assert_ne!(f.k.artifact(new_provider).unwrap(), old_artifact);
                f.k.issue_grant(f.caller, 3, authority(Access::Write, &["item"]))
                    .unwrap();
            }
            _ => unreachable!(),
        }
        f.refuses("commit", &id);
    }
}
#[test]
fn duplicate_participants_hidden_reads_undeclared_owner_and_validation_fail_prepare() {
    let f = Fixture::new();
    let mut plans = vec![];
    let mut p = f.plan();
    p.readonly.push(f.stamps[0].clone());
    plans.push(p);
    let mut p = f.plan();
    p.writes.push(p.writes[0].clone());
    plans.push(p);
    let mut p = f.plan();
    p.writes[0].stamp.address.owner = "invented".into();
    plans.push(p);
    let mut p = f.plan();
    p.writes[0].stamp.revision += 1;
    plans.push(p);
    let mut p = f.plan();
    p.writes[0].stamp.generation += 1;
    plans.push(p);
    let mut p = f.plan();
    p.writes[0].stamp = f.stamps[4].clone();
    p.readonly.clear();
    plans.push(p);
    for p in plans {
        f.refuses("prepare", &p);
    }
    f.owner_valid.set(false);
    f.refuses("prepare", &f.plan());
    f.owner_valid.set(true);
    f.accounting_valid.set(false);
    f.refuses("prepare", &f.plan());
}
#[test]
fn another_caller_host_replaced_caller_and_service_cannot_adopt_plan() {
    let mut f = Fixture::new();
    let id = f.prepare(&f.plan());
    let other = guest(&mut f.k, "other", &f.owners, &["rules"]);
    f.grants(other);
    let before = f.service.state().unwrap();
    assert!(call::<_, Identity>(
        &f.k,
        other,
        "commit",
        &id,
        authority(Access::Write, &f.owners)
    )
    .is_err());
    assert_eq!(f.service.state().unwrap(), before);
    let old_artifact = f.k.artifact(f.caller).unwrap();
    f.k.unregister(f.caller).unwrap();
    f.caller = guest_code(
        &mut f.k,
        "caller",
        &f.owners,
        &["rules"],
        "(func $newcode nop)",
    );
    assert_ne!(f.k.artifact(f.caller).unwrap(), old_artifact);
    f.grants(f.caller);
    f.refuses("commit", &id);
    f.k.unregister(f.service_handle).unwrap();
    let second = Service::new(
        Limits {
            wire: WIRE,
            ..Limits::default()
        },
        Rc::new(|_| true),
    )
    .unwrap();
    second.install(&mut f.k, "resources").unwrap();
    f.grants(f.caller);
    assert!(f.commit(&id).is_err());
    assert_eq!(f.service.state().unwrap(), before);
    assert_eq!(second.state().unwrap().next_serial, 1);
    let mut foreign = Fixture::new();
    f.refuses("commit", &id);
    foreign.grants(foreign.caller);
    foreign.refuses("commit", &id);
}
#[test]
fn admission_scope_ancestor_access_and_reply_preflight_preserve_state() {
    let mut f = Fixture::new();
    let before = f.service.state().unwrap();
    for auth in [
        authority(Access::Write, &["rules"]),
        authority(Access::Read, &f.owners),
    ] {
        assert!(call::<_, Identity>(&f.k, f.caller, "prepare", &f.plan(), auth).is_err());
        assert_eq!(f.service.state().unwrap(), before);
    }
    f.refuses("small", &f.plan());
    // Coordinator admission alone is insufficient to read or write an owner.
    f.k.revoke_grant(f.caller, 3).unwrap();
    f.refuses("read", &address("item"));
    f.refuses("prepare", &f.plan());
    // Unrelated grant revocation does not invalidate a prepared operation.
    f.k.issue_grant(f.caller, 3, authority(Access::Write, &["item"]))
        .unwrap();
    let id = f.prepare(&f.plan());
    f.k.revoke_grant(f.caller, 8).unwrap();
    assert_eq!(f.commit(&id), Ok(id));
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct Debt {
    quantity: u64,
    terms: String,
    creditor: String,
    liability: i64,
    consumed: bool,
}
#[test]
fn generic_item_equipment_timer_cleanup_retains_consumed_debt_and_prefix() {
    let mut k = Kernel::new(KernelLimits::default()).unwrap();
    let original = Debt {
        quantity: 2,
        terms: "agreed-price-17".into(),
        creditor: "shop-9".into(),
        liability: 34,
        consumed: false,
    };
    let expected = original.clone();
    let service = Service::new(
        Limits {
            wire: WIRE,
            ..Limits::default()
        },
        Rc::new(move |d| {
            if d.plan.writes.is_empty() {
                return d.plan.readonly.is_empty();
            }
            if d.snapshots.len() != 4 || d.plan.writes.len() != 4 {
                return false;
            }
            for owner in ["item", "equip", "timer"] {
                if !d
                    .plan
                    .writes
                    .iter()
                    .any(|w| w.stamp.address == address(owner) && w.replacement.is_none())
                {
                    return false;
                }
            }
            let Some(old) = d
                .snapshots
                .iter()
                .find(|s| s.stamp.address == address("debt"))
            else {
                return false;
            };
            let Some(new) = d
                .plan
                .writes
                .iter()
                .find(|w| w.stamp.address == address("debt"))
                .and_then(|w| w.replacement.as_ref())
            else {
                return false;
            };
            let Ok(old_debt) = serde_json::from_slice::<Debt>(&old.value) else {
                return false;
            };
            let Ok(new_debt) = serde_json::from_slice::<Debt>(new) else {
                return false;
            };
            let mut consumed = expected.clone();
            consumed.consumed = true;
            old_debt == expected
                && new_debt == consumed
                && d.snapshots
                    .iter()
                    .any(|s| s.stamp.address == address("item") && s.value == b"quantity=2")
                && d.snapshots
                    .iter()
                    .any(|s| s.stamp.address == address("equip") && s.value == b"item-reference")
                && d.snapshots
                    .iter()
                    .any(|s| s.stamp.address == address("timer") && s.value == b"item-reference")
        }),
    )
    .unwrap();
    service.install(&mut k, "resources").unwrap();
    let owners = ["item", "equip", "timer", "debt", "unrelated"];
    for owner in owners {
        let h = provider(&mut k, owner);
        service
            .register_owner(
                owner,
                OwnerPolicy {
                    capability: cap(owner),
                    provider: h,
                    artifact: k.artifact(h).unwrap(),
                    validate: Rc::new(|_| true),
                },
            )
            .unwrap();
    }
    let values = [
        b"quantity=2".to_vec(),
        b"item-reference".to_vec(),
        b"item-reference".to_vec(),
        serde_json::to_vec(&original).unwrap(),
        b"untouched".to_vec(),
    ];
    let stamps: Vec<_> = owners
        .iter()
        .zip(values)
        .map(|(o, v)| service.put(address(o), v).unwrap())
        .collect();
    let caller = guest(&mut k, "caller", &owners, &[]);
    for slot in 0..owners.len() + 3 {
        k.issue_grant(
            caller,
            slot,
            authority(
                if slot == 0 {
                    Access::Read
                } else {
                    Access::Write
                },
                &owners,
            ),
        )
        .unwrap();
    }
    let invoke = |endpoint: &str, p: &serde_json::Value| {
        call::<_, serde_json::Value>(&k, caller, endpoint, p, authority(Access::Write, &owners))
    };
    let prefix = Plan {
        readonly: vec![],
        writes: vec![],
        events: vec![b"prefix".to_vec()],
    };
    let id = invoke("prepare", &serde_json::to_value(prefix).unwrap()).unwrap();
    invoke("commit", &id).unwrap();
    let before = service.state().unwrap();
    let mut consumed = original.clone();
    consumed.consumed = true;
    let mut plan = Plan {
        readonly: vec![],
        writes: stamps[..3]
            .iter()
            .map(|s| Write {
                stamp: s.clone(),
                replacement: None,
            })
            .collect(),
        events: vec![b"cleanup".to_vec(), b"debt-consumed".to_vec()],
    };
    plan.writes.push(Write {
        stamp: stamps[3].clone(),
        replacement: Some(serde_json::to_vec(&consumed).unwrap()),
    });
    let mut invalid = plan.clone();
    consumed.liability -= 1;
    invalid.writes[3].replacement = Some(serde_json::to_vec(&consumed).unwrap());
    assert!(invoke("prepare", &serde_json::to_value(invalid).unwrap()).is_err());
    assert_eq!(service.state().unwrap(), before);
    let id = invoke("prepare", &serde_json::to_value(plan).unwrap()).unwrap();
    invoke("commit", &id).unwrap();
    let after = service.state().unwrap();
    for o in ["item", "equip", "timer"] {
        assert!(!after.resources.contains_key(&address(o)));
    }
    let debt: Debt = serde_json::from_slice(&after.resources[&address("debt")].value).unwrap();
    let mut expected = original;
    expected.consumed = true;
    assert_eq!(debt, expected);
    assert_eq!(
        after.resources[&address("unrelated")],
        before.resources[&address("unrelated")]
    );
    assert_eq!(
        after.history,
        vec![
            b"prefix".to_vec(),
            b"cleanup".to_vec(),
            b"debt-consumed".to_vec()
        ]
    );
    // A later refusal preserves the earlier completed transaction prefix.
    assert!(invoke("commit", &id).is_err());
    assert_eq!(service.state().unwrap(), after);
}
#[test]
fn bounded_plans_payloads_pending_history_and_live_bookkeeping() {
    for kind in ["participants", "payload", "events", "pending", "history"] {
        let mut limits = Limits {
            wire: WIRE,
            ..Limits::default()
        };
        match kind {
            "participants" => limits.participants = 1,
            "payload" => limits.payload = 16,
            "events" => limits.events = 0,
            "pending" => limits.pending = 1,
            "history" => limits.history = 0,
            _ => unreachable!(),
        }
        let f = Fixture::with_limits(limits);
        let mut plan = f.plan();
        match kind {
            "payload" => {
                plan.writes[0].replacement = Some(vec![0; 17]);
                f.refuses("prepare", &plan);
                plan.writes[0].replacement = None;
                plan.events = vec![vec![0; 17]];
                f.refuses("prepare", &plan);
            }
            "pending" => {
                f.prepare(&plan);
                f.refuses("prepare", &plan);
            }
            "history" => {
                let id = f.prepare(&plan);
                f.refuses("commit", &id);
            }
            _ => f.refuses("prepare", &plan),
        }
    }
    let f = Fixture::with_limits(Limits {
        owners: 6,
        resources: 6,
        wire: WIRE,
        ..Limits::default()
    });
    let before = f.service.state().unwrap();
    assert_eq!(
        f.service.put(
            Resource {
                owner: "item".into(),
                key: "other".into()
            },
            vec![]
        ),
        Err(Error::Limit)
    );
    let policy = OwnerPolicy {
        capability: cap("item"),
        provider: f.providers[0],
        artifact: f.k.artifact(f.providers[0]).unwrap(),
        validate: Rc::new(|_| true),
    };
    assert_eq!(
        f.service.register_owner("another", policy),
        Err(Error::Limit)
    );
    assert_eq!(f.service.state().unwrap(), before);
}
#[test]
fn reentrant_validator_port_and_admin_access_refuse_without_partial_mutation() {
    use std::cell::RefCell;
    struct Context {
        kernel: Rc<Kernel>,
        service: Service,
        caller: ModuleHandle,
    }
    let context: Rc<RefCell<Option<Context>>> = Rc::new(RefCell::new(None));
    let callback_context = context.clone();
    let checks = Rc::new(Cell::new(0));
    let counted = checks.clone();
    let service = Service::new(
        Limits {
            wire: WIRE,
            ..Limits::default()
        },
        Rc::new(move |_| {
            let borrowed = callback_context.borrow();
            let c = borrowed.as_ref().unwrap();
            assert_eq!(
                c.service.put(address("item"), b"partial".to_vec()),
                Err(Error::NativeRefusal)
            );
            assert_eq!(
                c.service.delete(&address("item")),
                Err(Error::NativeRefusal)
            );
            assert_eq!(c.service.state(), Err(Error::NativeRefusal));
            let id = Identity {
                service: 1,
                serial: 1,
            };
            assert_eq!(
                call::<_, Identity>(
                    &c.kernel,
                    c.caller,
                    "commit",
                    &id,
                    authority(Access::Write, &["item"])
                ),
                Err(Error::NativeRefusal.abi_code())
            );
            counted.set(counted.get() + 1);
            true
        }),
    )
    .unwrap();
    let mut k = Kernel::new(KernelLimits::default()).unwrap();
    service.install(&mut k, "resources").unwrap();
    let h = provider(&mut k, "item");
    service
        .register_owner(
            "item",
            OwnerPolicy {
                capability: cap("item"),
                provider: h,
                artifact: k.artifact(h).unwrap(),
                validate: Rc::new(|_| true),
            },
        )
        .unwrap();
    let stamp = service.put(address("item"), b"old".to_vec()).unwrap();
    let caller = guest(&mut k, "caller", &["item"], &[]);
    for slot in 0..4 {
        k.issue_grant(
            caller,
            slot,
            authority(
                if slot == 0 {
                    Access::Read
                } else {
                    Access::Write
                },
                &["item"],
            ),
        )
        .unwrap();
    }
    let k = Rc::new(k);
    *context.borrow_mut() = Some(Context {
        kernel: k.clone(),
        service: service.clone(),
        caller,
    });
    let plan = Plan {
        readonly: vec![],
        writes: vec![Write {
            stamp,
            replacement: Some(b"new".to_vec()),
        }],
        events: vec![],
    };
    let before = service.state().unwrap();
    let id: Identity = call(
        &k,
        caller,
        "prepare",
        &plan,
        authority(Access::Write, &["item"]),
    )
    .unwrap();
    assert_eq!(service.state().unwrap().resources, before.resources);
    call::<_, Identity>(
        &k,
        caller,
        "commit",
        &id,
        authority(Access::Write, &["item"]),
    )
    .unwrap();
    assert_eq!(
        service.state().unwrap().resources[&address("item")].value,
        b"new"
    );
    assert_eq!(checks.get(), 2);
    context.borrow_mut().take();
}
#[test]
fn wrong_capability_version_and_foreign_host_refuse_through_runtime() {
    let mut f = Fixture::new();
    let before = f.service.state().unwrap();
    let foreign = Kernel::new(KernelLimits::default()).unwrap();
    assert_eq!(
        foreign.invoke(
            f.caller,
            "commit",
            b"{}",
            authority(Access::Write, &f.owners)
        ),
        Err(Error::InvalidHandle)
    );
    assert_eq!(f.service.state().unwrap(), before);
    // A currently registered v2 export does not satisfy the host's v1 policy.
    let mut m = manifest("versioned");
    let mut endpoint = ep(Access::Write);
    endpoint.major = 2;
    m.exports.insert("guard".into(), endpoint);
    let port: NativePort = Rc::new(|_, _| Ok(vec![]));
    let h =
        f.k.register_native(m, b"version-two", BTreeMap::from([("guard".into(), port)]))
            .unwrap();
    let expected = Capability {
        provider: "versioned".into(),
        endpoint: "guard".into(),
        major: 1,
    };
    f.service
        .register_owner(
            "versioned",
            OwnerPolicy {
                capability: expected.clone(),
                provider: h,
                artifact: f.k.artifact(h).unwrap(),
                validate: Rc::new(|_| true),
            },
        )
        .unwrap();
    let stamp = f
        .service
        .put(address("versioned"), b"old".to_vec())
        .unwrap();
    let mut m = manifest("version-caller");
    m.exports.insert("prepare".into(), ep(Access::Write));
    m.dependencies = vec![
        Dependency {
            capability: Capability {
                provider: "resources".into(),
                endpoint: "prepare".into(),
                major: 1,
            },
            access: Access::Write,
        },
        Dependency {
            capability: Capability {
                major: 2,
                ..expected
            },
            access: Access::Write,
        },
    ];
    let bytes = wat::parse_str(r#"(module
        (import "kernel" "call" (func $call (param i32 i32 i32 i32 i32) (result i32)))
        (memory (export "memory") 1) (func (export "alloc") (param i32) (result i32) i32.const 0)
        (func (export "prepare") (param i32 i32) (result i64)
            i32.const 32768 i32.const 0 local.get 0 local.get 1 i32.const 8192 i32.const 8192 call $call i32.store
            i64.const 140737488355332))"#).unwrap();
    let caller = f.k.register_guest(m, &bytes).unwrap();
    for slot in 0..2 {
        f.k.issue_grant(caller, slot, authority(Access::Write, &["versioned"]))
            .unwrap();
    }
    let before = f.service.state().unwrap();
    let plan = Plan {
        readonly: vec![],
        writes: vec![Write {
            stamp,
            replacement: None,
        }],
        events: vec![],
    };
    assert_eq!(
        call_raw(
            &f.k,
            caller,
            "prepare",
            &serde_json::to_vec(&plan).unwrap(),
            authority(Access::Write, &["versioned"])
        ),
        Err(Error::Authority.abi_code())
    );
    assert_eq!(f.service.state().unwrap(), before);
}
#[test]
fn nested_ancestor_scope_cannot_be_laundered_by_inner_grants() {
    let mut f = Fixture::new();
    let mut outer = manifest("outer");
    outer.exports.insert("run".into(), ep(Access::Write));
    outer.dependencies.push(Dependency {
        capability: Capability {
            provider: "caller".into(),
            endpoint: "prepare".into(),
            major: 1,
        },
        access: Access::Write,
    });
    let bytes = wat::parse_str(
        r#"(module
        (import "kernel" "call" (func $call (param i32 i32 i32 i32 i32) (result i32)))
        (memory (export "memory") 1) (func (export "alloc") (param i32) (result i32) i32.const 0)
        (func (export "run") (param i32 i32) (result i64)
            i32.const 0 local.get 0 local.get 1 i32.const 32768 i32.const 8192 call $call drop
            i64.const 140737488355332))"#,
    )
    .unwrap();
    let outer = f.k.register_guest(outer, &bytes).unwrap();
    f.k.issue_grant(outer, 0, authority(Access::Write, &["rules"]))
        .unwrap();
    let before = f.service.state().unwrap();
    assert_eq!(
        call_raw(
            &f.k,
            outer,
            "run",
            &serde_json::to_vec(&f.plan()).unwrap(),
            authority(Access::Write, &f.owners)
        ),
        Err(Error::Authority.abi_code())
    );
    assert_eq!(f.service.state().unwrap(), before);
    f.refuses(
        "read",
        &serde_json::json!({"owner":"item", "key":"value", "caller":"forged"}),
    );
}

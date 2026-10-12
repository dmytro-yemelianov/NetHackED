use nethacked_kernel::{
    Access, Authority, Capability, Dependency, Endpoint, Error, Extension, Kernel,
    Limits as KernelLimits, Manifest, ModuleHandle, NativePort, PipelineLease, Resource, Scope,
};
use nethacked_resources::*;
use std::{
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
fn cap(owner: &str, endpoint: &str) -> Capability {
    Capability {
        provider: owner.into(),
        endpoint: endpoint.into(),
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
fn authority(access: Access, owners: &[&str]) -> Authority {
    Authority {
        access,
        scope: Scope::Keys(owners.iter().map(|owner| address(owner)).collect()),
    }
}
fn decode(reply: nethacked_kernel::Result<Vec<u8>>) -> std::result::Result<Vec<u8>, i32> {
    let reply = reply.map_err(Error::abi_code)?;
    if reply.len() == 4 {
        let n = i32::from_le_bytes(reply.as_slice().try_into().unwrap());
        if n < 0 {
            return Err(n);
        }
    }
    Ok(reply)
}
struct Fixture {
    k: Kernel,
    service: Service,
    caller: ModuleHandle,
    owners: Vec<ModuleHandle>,
    stamps: Vec<Stamp>,
}
impl Fixture {
    fn new() -> Self {
        let mut k = Kernel::new(KernelLimits::default()).unwrap();
        let service = Service::new(
            Limits {
                wire: WIRE,
                ..Limits::default()
            },
            Rc::new(|_| true),
        )
        .unwrap();
        service.install(&mut k, "resources").unwrap();
        let mut owners = vec![];
        let mut stamps = vec![];
        for owner in ["item", "rules", "unrelated"] {
            let mut m = manifest(owner);
            m.exports.insert("guard".into(), ep(Access::Write));
            m.extension_points.insert("guard".into());
            let p: NativePort = Rc::new(|_, _| Ok(vec![]));
            let h = k
                .register_native(m, owner.as_bytes(), BTreeMap::from([("guard".into(), p)]))
                .unwrap();
            service
                .register_owner(
                    owner,
                    OwnerPolicy {
                        capability: cap(owner, "guard"),
                        provider: h,
                        artifact: k.artifact(h).unwrap(),
                        validate: Rc::new(|_| true),
                    },
                )
                .unwrap();
            owners.push(h);
            stamps.push(
                service
                    .put(address(owner), owner.as_bytes().to_vec())
                    .unwrap(),
            );
        }
        let mut m = manifest("caller");
        for (name, access) in [
            ("read", Access::Read),
            ("prepare", Access::Write),
            ("commit", Access::Write),
        ] {
            m.exports.insert(name.into(), ep(access));
            m.dependencies.push(Dependency {
                capability: cap("resources", name),
                access,
            });
        }
        // rules is a permission-only Read dependency to a Write-capable owner.
        for (owner, access) in [("item", Access::Write), ("rules", Access::Read)] {
            m.dependencies.push(Dependency {
                capability: cap(owner, "guard"),
                access,
            });
        }
        let mut wrappers = String::new();
        for (slot, name) in ["read", "prepare", "commit"].iter().enumerate() {
            wrappers.push_str(&format!(
                r#"(func (export "{name}") (param i32 i32) (result i64)
            i32.const {slot} local.get 0 local.get 1 call $forward)"#
            ));
        }
        let bytes = wat::parse_str(format!(r#"(module
          (import "kernel" "call" (func $call (param i32 i32 i32 i32 i32) (result i32)))
          (memory (export "memory") 1)
          (func (export "alloc") (param i32) (result i32) i32.const 0)
          (func $forward (param $slot i32) (param $p i32) (param $n i32) (result i64) (local $r i32)
            local.get $slot local.get $p local.get $n i32.const 32768 i32.const {WIRE} call $call local.set $r
            local.get $r i32.const 0 i32.lt_s if i32.const 32768 local.get $r i32.store i32.const 4 local.set $r end
            i64.const 140737488355328 local.get $r i64.extend_i32_u i64.or)
          {wrappers})"#)).unwrap();
        let caller = k.register_guest(m, &bytes).unwrap();
        for slot in 0..5 {
            k.issue_grant(
                caller,
                slot,
                authority(
                    if slot == 0 || slot == 4 {
                        Access::Read
                    } else {
                        Access::Write
                    },
                    &["item", "rules"],
                ),
            )
            .unwrap();
        }
        Self {
            k,
            service,
            caller,
            owners,
            stamps,
        }
    }
    fn plan(&self) -> Plan {
        Plan {
            readonly: vec![self.stamps[1].clone()],
            writes: vec![Write {
                stamp: self.stamps[0].clone(),
                replacement: Some(b"changed".to_vec()),
            }],
            events: vec![b"committed".to_vec()],
        }
    }
    fn capture(&mut self, endpoint: &str) -> PipelineLease {
        self.k
            .capture_pipeline(
                self.caller,
                endpoint,
                authority(Access::Write, &["item", "rules"]),
            )
            .unwrap()
    }
    fn retained<T: serde::Serialize>(
        &self,
        lease: PipelineLease,
        input: &T,
    ) -> std::result::Result<Vec<u8>, i32> {
        decode(self.k.invoke_pipeline(
            lease,
            &serde_json::to_vec(input).unwrap(),
            authority(Access::Write, &["item", "rules"]),
        ))
    }
    fn ordinary<T: serde::Serialize>(
        &self,
        endpoint: &str,
        input: &T,
    ) -> std::result::Result<Vec<u8>, i32> {
        decode(self.k.invoke(
            self.caller,
            endpoint,
            &serde_json::to_vec(input).unwrap(),
            authority(Access::Write, &["item", "rules"]),
        ))
    }
    fn change(&mut self, owner: &str) -> ModuleHandle {
        let mut m = manifest(&format!("new-{owner}"));
        m.exports.insert("guard".into(), ep(Access::Write));
        m.extension = Some(Extension {
            target: cap(owner, "guard"),
            export: "guard".into(),
        });
        let bytes = wat::parse_str(r#"(module (memory (export "memory") 1)
          (func (export "alloc") (param i32) (result i32) i32.const 0)
          (func (export "guard") (param i32 i32) (result i64) i32.const 128 i32.const 9 i32.store i64.const 549755813892))"#).unwrap();
        let h = self.k.register_guest(m, &bytes).unwrap();
        assert_eq!(
            self.k
                .invoke(
                    self.owners[if owner == "item" { 0 } else { 1 }],
                    "guard",
                    &[],
                    authority(Access::Write, &[owner])
                )
                .unwrap(),
            9u32.to_le_bytes()
        );
        h
    }
}

#[test]
fn real_prepare_commit_retains_write_and_permission_only_readonly_chains() {
    let mut f = Fixture::new();
    let prepare = f.capture("prepare");
    let commit = f.capture("commit");
    let initial = f.service.state().unwrap();
    let id: Identity = serde_json::from_slice(&f.retained(prepare, &f.plan()).unwrap()).unwrap();
    let pending = f.service.state().unwrap();
    assert_eq!(pending.resources, initial.resources);
    assert_eq!(pending.history, initial.history);
    let item_ext = f.change("item");
    let rules_ext = f.change("rules");
    let before = f.service.state().unwrap();
    assert_eq!(f.ordinary("commit", &id), Err(Error::Authority.abi_code()));
    assert_eq!(f.service.state().unwrap(), before);
    for h in &f.owners[..2] {
        assert_eq!(f.k.unregister(*h), Err(Error::Referenced));
    }
    // New captures use the changed chain, and this same still-live ID refuses.
    let new_commit = f.capture("commit");
    assert_eq!(
        f.retained(new_commit, &id),
        Err(Error::Authority.abi_code())
    );
    assert_eq!(f.service.state().unwrap(), before);
    assert_eq!(
        f.retained(commit, &id).unwrap(),
        serde_json::to_vec(&serde_json::to_value(&id).unwrap()).unwrap()
    );
    let after = f.service.state().unwrap();
    assert_eq!(after.commits, initial.commits + 1);
    assert_eq!(after.history, vec![b"committed".to_vec()]);
    assert_eq!(after.resources[&address("item")].value, b"changed");
    assert_eq!(
        after.resources[&address("rules")],
        initial.resources[&address("rules")]
    );
    assert_eq!(
        after.resources[&address("unrelated")],
        initial.resources[&address("unrelated")]
    );
    // A new valid prepare/commit succeeds on the new pipeline as well.
    let mut plan = f.plan();
    plan.writes[0].stamp = after.resources[&address("item")].stamp.clone();
    let new_prepare = f.capture("prepare");
    let id2: Identity = serde_json::from_slice(&f.retained(new_prepare, &plan).unwrap()).unwrap();
    f.retained(new_commit, &id2).unwrap();
    f.k.managed_detach(item_ext).unwrap();
    f.k.managed_detach(rules_ext).unwrap();
    assert_eq!(f.k.managed_unload(rules_ext), Err(Error::Referenced));
    f.k.release_pipeline(new_prepare).unwrap();
    f.k.release_pipeline(new_commit).unwrap();
    f.k.managed_unload(item_ext).unwrap();
    f.k.managed_unload(rules_ext).unwrap();
}

#[test]
fn live_retained_resource_plans_recheck_revoke_narrow_and_ancestor_authority() {
    for defect in [
        "revoke-write",
        "revoke-read",
        "narrow-write",
        "narrow-read",
        "read-access",
        "ancestor",
    ] {
        let mut f = Fixture::new();
        let prepare = f.capture("prepare");
        let commit = f.capture("commit");
        let id: Identity =
            serde_json::from_slice(&f.retained(prepare, &f.plan()).unwrap()).unwrap();
        f.change("item");
        f.change("rules");
        let before = f.service.state().unwrap();
        let registry = f.k.lifecycle_state();
        let slot = if defect.ends_with("read") { 4 } else { 3 };
        match defect {
            "revoke-write" | "revoke-read" => f.k.revoke_grant(f.caller, slot).unwrap(),
            "narrow-write" | "narrow-read" => {
                f.k.issue_grant(
                    f.caller,
                    slot,
                    authority(
                        if slot == 4 {
                            Access::Read
                        } else {
                            Access::Write
                        },
                        &[],
                    ),
                )
                .unwrap()
            }
            "read-access" => {
                f.k.issue_grant(f.caller, 3, authority(Access::Read, &["item"]))
                    .unwrap()
            }
            "ancestor" => (),
            _ => unreachable!(),
        }
        let refused = if defect == "ancestor" {
            decode(f.k.invoke_pipeline(
                commit,
                &serde_json::to_vec(&id).unwrap(),
                authority(Access::Write, &["item"]),
            ))
        } else {
            f.retained(commit, &id)
        };
        assert_eq!(refused, Err(Error::Authority.abi_code()), "{defect}");
        assert_eq!(f.service.state().unwrap(), before, "{defect}");
        assert_eq!(f.k.lifecycle_state(), registry);
        // Restore the changed grant, never replace or consume the tested ID.
        if defect != "ancestor" {
            f.k.issue_grant(
                f.caller,
                slot,
                authority(
                    if slot == 4 {
                        Access::Read
                    } else {
                        Access::Write
                    },
                    &["item", "rules"],
                ),
            )
            .unwrap();
        }
        f.retained(commit, &id).unwrap();
        assert_eq!(f.service.state().unwrap().commits, 1);
    }
}

#[test]
fn missing_permission_at_capture_cannot_become_resource_authority() {
    let mut f = Fixture::new();
    f.k.revoke_grant(f.caller, 4).unwrap();
    let absent = f.capture("prepare");
    f.k.issue_grant(f.caller, 4, authority(Access::Read, &["rules"]))
        .unwrap();
    let before = f.service.state().unwrap();
    assert_eq!(
        f.retained(absent, &f.plan()),
        Err(Error::Authority.abi_code())
    );
    assert_eq!(f.service.state().unwrap(), before);
    let valid = f.capture("prepare");
    let commit = f.capture("commit");
    let id: Identity = serde_json::from_slice(&f.retained(valid, &f.plan()).unwrap()).unwrap();
    f.retained(commit, &id).unwrap();
}

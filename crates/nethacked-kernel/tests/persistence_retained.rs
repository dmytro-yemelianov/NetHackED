#[path = "support/persistence_retained.rs"]
mod support;
use nethacked_kernel::{persistence::*, *};
use support::*;

fn module(s: &KernelSnapshot, name: &str) -> u64 {
    s.registry
        .modules
        .iter()
        .find(|r| r.manifest.id == name)
        .unwrap()
        .id
}
#[test]
fn complete_old_graph_source_free_restart_continuation_and_shared_lifetimes() {
    let mut f = fixture();
    let stale = f.operation.clone();
    let stale_handle = f.actor;
    let stale_callback = f.callback.clone();
    let stale_owner = f.owner;
    let stale_pure = f.host.effects.pure.borrow().clone().unwrap();
    let stale_cleanup = f.cleanup;
    assert_ne!(f.old, f.new);
    assert!(f.kernel.pipeline_descriptor(f.disposed).is_err());
    let before = f.kernel.lifecycle_state();
    let effects = f.host.effects.calls.get();
    assert!(matches!(
        f.kernel.registry_snapshot(),
        Err(Error::Referenced)
    ));
    let s = encoded(&saved(&mut f));
    assert_eq!(f.kernel.lifecycle_state(), before);
    assert_eq!(f.host.effects.calls.get(), effects);
    assert_eq!(s.disposed_sources.len(), 1);
    assert_eq!(s.callbacks.len(), 1);
    assert_eq!(s.pins.len(), 2);
    // No source runtime or native closures participate in reconstruction.
    let catalog = f.host.materials.clone();
    drop(f);
    let mut host = Host::approved(catalog.clone(), &s);
    host.resource_owner = "restart".into();
    let live_auth = host.authority();
    let mut k = Kernel::new(Limits::default()).unwrap();
    let restored = k.restore_kernel(&s, &host).unwrap();
    assert_eq!(host.effects.calls.get(), 0);
    assert_ne!(
        k.pipeline_descriptor(restored.operations()[0].pipeline())
            .unwrap()
            .hash,
        s.leases[0].pipeline.hash
    );
    assert_eq!(host.effects.callbacks.borrow().len(), 0);
    assert!(host.effects.observed.borrow().is_empty());
    assert_eq!(host.factories.get(), s.registry.modules.len());
    assert_eq!(
        k.lifecycle_state().next_retention,
        s.registry.retention_counters[0]
    );
    assert_eq!(
        k.lifecycle_state().next_callback,
        s.registry.retention_counters[1]
    );
    assert_eq!(
        k.lifecycle_state().next_generation,
        s.registry.next_generation
    );
    assert_ne!(
        restored.operations()[0].operation().namespace(),
        s.source_namespace
    );
    assert_eq!(restored.operations()[0].checkpoint(), 9);
    assert_eq!(restored.operations()[0].phase(), "paused");
    assert!(!restored.operations()[0].admits_cleanup(stale_cleanup));
    assert!(restored.operations()[0].admits_cleanup(*restored.cleanup().values().next().unwrap()));
    assert!(k
        .invoke(stale_handle, "resume", &bytes(5), live_auth.clone())
        .is_err());
    assert!(k
        .invoke_operation(&stale, &bytes(5), live_auth.clone())
        .is_err());
    assert!(k.adopt_callback(&stale_callback).is_err());
    assert!(k.release_owner(stale_owner).is_err());
    assert_eq!(host.effects.calls.get(), 0);
    let pure = restored.pure_captures()[0].clone();
    *host.effects.pure.borrow_mut() = Some(pure.clone());
    let retained = restored.operations()[0].clone();
    assert_eq!(
        k.invoke_operation(&retained, &bytes(5), live_auth.clone())
            .unwrap(),
        bytes(116)
    );
    let observed = host.effects.observed.borrow()[0].clone();
    assert_eq!(observed, retained);
    *host.effects.pure.borrow_mut() = Some(stale_pure);
    assert!(k
        .invoke_operation(&retained, &bytes(5), live_auth.clone())
        .is_err());
    *host.effects.pure.borrow_mut() = Some(pure.clone());
    assert_eq!(
        k.invoke(
            restored.modules()[&module(&s, "actor")],
            "resume",
            &bytes(5),
            live_auth.clone()
        )
        .unwrap(),
        bytes(236)
    );
    let callback = restored.callbacks().values().next().unwrap().clone();
    let shared_callback = callback.clone();
    let pin = restored.pins().values().next().unwrap().clone();
    let shared_pin = pin.clone();
    let count = k.lifecycle_state();
    drop(callback);
    drop(pin);
    assert_eq!(k.lifecycle_state(), count);
    assert!(k
        .invoke_pipeline(
            *restored.historical_sources().values().next().unwrap(),
            &bytes(5),
            live_auth.clone()
        )
        .is_err());
    let (op, lease) = k.adopt_callback(&shared_callback).unwrap();
    let adopted = k
        .operation_binding_with_cleanup(
            op,
            lease,
            10,
            "continue",
            &[*restored.cleanup().values().next().unwrap()],
        )
        .unwrap();
    assert_eq!(
        k.invoke_operation(&adopted, &bytes(5), live_auth.clone())
            .unwrap(),
        bytes(116)
    );
    let origin = k.callback_provenance(lease).unwrap().unwrap();
    assert_eq!(origin.source.checkpoint(), 7);
    assert_eq!(origin.source.phase(), "captured");
    assert_ne!(origin.source, stale_callback.provenance().source);
    // Repeated save/rebuild of an already adopted callback preserves historical
    // source tombstones and the old route, not today's helper/native dispatch.
    let second = encoded(
        &k.kernel_snapshot(&KernelEnrollment {
            operations: &[retained.clone(), adopted.clone()],
            pure_captures: &[pure.clone()],
        })
        .unwrap(),
    );
    let mut restart = Kernel::new(Limits::default()).unwrap();
    let mut host2 = Host::approved(catalog, &second);
    host2.resource_owner = "second".into();
    let second_auth = host2.authority();
    let fresh = restart.restore_kernel(&second, &host2).unwrap();
    *host2.effects.pure.borrow_mut() = Some(fresh.pure_captures()[0].clone());
    assert_eq!(
        restart
            .invoke_operation(&fresh.operations()[0], &bytes(5), second_auth.clone())
            .unwrap(),
        bytes(116)
    );
    assert_eq!(
        restart
            .invoke_operation(&fresh.operations()[1], &bytes(5), second_auth.clone())
            .unwrap(),
        bytes(116)
    );
    assert!(restart
        .invoke_operation(&adopted, &bytes(5), second_auth.clone())
        .is_err());
    let old = restored.modules()[&module(&s, "old-helper")];
    assert_eq!(k.managed_unload(old), Err(Error::Referenced));
    let owner = *restored.owners().values().next().unwrap();
    k.release_owner(owner).unwrap();
    k.release_pipeline(retained.pipeline()).unwrap();
    k.release_pipeline(lease).unwrap();
    drop(restored);
    drop(pure);
    host.effects.pure.borrow_mut().take();
    assert_eq!(k.managed_unload(old), Err(Error::Referenced));
    drop(shared_pin);
    assert_eq!(k.managed_unload(old), Err(Error::Referenced));
    drop(shared_callback);
    assert_eq!(k.lifecycle_state().references, 0);
    k.managed_unload(old).unwrap();
    assert_eq!(
        k.invoke(
            k_handle(&second, &fresh, "actor"),
            "resume",
            &bytes(5),
            live_auth.clone()
        ),
        Err(Error::InvalidHandle)
    );
    let fresh_actor = k_handle(&second, &fresh, "actor");
    assert_eq!(
        restart
            .invoke(fresh_actor, "resume", &bytes(5), second_auth.clone())
            .unwrap(),
        bytes(236)
    );
    // Runtime authority continues to be live, even after complete admission.
    let codec = k_handle(&second, &fresh, "codec");
    restart.revoke_grant(codec, 0).unwrap();
    let n = host2.effects.calls.get();
    assert!(restart
        .invoke_operation(&fresh.operations()[0], &bytes(5), second_auth.clone())
        .is_err());
    assert_eq!(host2.effects.calls.get(), n);
}
fn k_handle(s: &KernelSnapshot, r: &RestoredKernel, id: &str) -> ModuleHandle {
    r.modules()[&module(s, id)]
}

fn refuse(mut mutate: impl FnMut(&mut serde_json::Value), host_mutate: impl FnOnce(&mut Host)) {
    let mut source = fixture();
    let snapshot = saved(&mut source);
    let source_state = source.kernel.lifecycle_state();
    let source_effects = source.host.effects.calls.get();
    let mut populated = fixture();
    let before = populated.kernel.lifecycle_state();
    let old_binding = populated.operation.clone();
    let old_callback = populated.callback.clone();
    let old_pin = populated.host.effects.pin.borrow().clone().unwrap();
    let dest_effects = populated.host.effects.calls.get();
    let mut host = Host::approved(source.host.materials.clone(), &snapshot);
    host_mutate(&mut host);
    let mut value = serde_json::to_value(&snapshot).unwrap();
    mutate(&mut value);
    let encoded_bytes = serde_json::to_vec(&value).unwrap();
    let malformed: KernelSnapshot = serde_json::from_slice(&encoded_bytes).unwrap();
    assert!(
        populated.kernel.restore_kernel(&malformed, &host).is_err(),
        "accepted mutation"
    );
    assert_eq!(populated.kernel.lifecycle_state(), before);
    assert_eq!(host.effects.calls.get(), 0);
    assert_eq!(host.effects.callbacks.borrow().len(), 0);
    assert_eq!(populated.host.effects.calls.get(), dest_effects);
    assert_eq!(
        old_pin.module_count(),
        populated
            .host
            .effects
            .pin
            .borrow()
            .as_ref()
            .unwrap()
            .module_count()
    );
    assert_eq!(
        populated
            .kernel
            .invoke_operation(&old_binding, &bytes(5), auth())
            .unwrap(),
        bytes(116)
    );
    populated
        .kernel
        .preflight_callback(&old_callback, &bytes(5), &auth(), "retry", &[])
        .unwrap();
    assert_eq!(source.kernel.lifecycle_state(), source_state);
    assert_eq!(source.host.effects.calls.get(), source_effects);
    // Positive control from the same original encoded bytes and fresh factories.
    let positive = Host::approved(source.host.materials.clone(), &snapshot);
    let result = populated
        .kernel
        .restore_kernel(&encoded(&snapshot), &positive)
        .unwrap();
    assert!(populated
        .kernel
        .invoke_operation(&old_binding, &bytes(5), auth())
        .is_err());
    assert!(populated.kernel.adopt_callback(&old_callback).is_err());
    assert_eq!(positive.effects.calls.get(), 0);
    assert_eq!(
        populated
            .kernel
            .invoke_operation(&result.operations()[0], &bytes(5), auth())
            .unwrap(),
        bytes(116)
    );
}
#[test]
fn malformed_crosslinks_hashes_origins_counters_and_closures_refuse_atomically() {
    type Mutation = fn(&mut serde_json::Value);
    let mutations: &[(&str, Mutation)] = &[
        ("root", |s| {
            s["leases"][0]["pipeline"]["root"]["module"] = 999u64.into()
        }),
        ("route", |s| {
            s["leases"][0]["pipeline"]["routing"]["routes"]
                .as_array_mut()
                .unwrap()
                .pop();
        }),
        ("slot", |s| {
            s["leases"][0]["pipeline"]["routing"]["slots"]
                .as_array_mut()
                .unwrap()
                .pop();
        }),
        ("hash", |s| {
            s["leases"][0]["pipeline"]["hash"][0] = 99u64.into()
        }),
        ("origin-service", |s| {
            s["callbacks"][0]["pipeline"]["origin"]["service"]["module"] = 999u64.into()
        }),
        ("origin-artifact", |s| {
            s["callbacks"][0]["pipeline"]["origin"]["service"]["artifact"][0] = 99u64.into()
        }),
        ("origin-caller", |s| {
            s["callbacks"][0]["pipeline"]["origin"]["caller"]["endpoint"] = "run".into()
        }),
        ("origin-slot", |s| {
            s["callbacks"][0]["pipeline"]["origin"]["slot"] = 1u64.into()
        }),
        ("origin-capability", |s| {
            s["callbacks"][0]["pipeline"]["origin"]["capability"]["major"] = 2u64.into()
        }),
        ("origin-source", |s| {
            s["callbacks"][0]["pipeline"]["origin"]["source"]["pipeline"] = 1u64.into()
        }),
        ("historical-route", |s| {
            s["callbacks"][0]["pipeline"]["origin"]["source_pipeline"]["routing"]["routes"]
                .as_array_mut()
                .unwrap()
                .pop();
        }),
        ("operation", |s| {
            s["operations"][0]["operation"] = s["registry"]["next_operation"].clone()
        }),
        ("operation-live-edge", |s| {
            s["operations"][0]["pipeline"] = 1u64.into()
        }),
        ("cleanup", |s| {
            s["operations"][0]["cleanup"][0] = 0u64.into()
        }),
        ("lease-highwater", |s| {
            s["registry"]["retention_counters"][0] = 1u64.into()
        }),
        ("callback-highwater", |s| {
            s["registry"]["retention_counters"][1] = 1u64.into()
        }),
        ("pin-highwater", |s| {
            s["registry"]["retention_counters"][2] = 1u64.into()
        }),
        ("duplicate-pin", |s| {
            let row = s["pins"][0].clone();
            s["pins"].as_array_mut().unwrap().push(row);
        }),
        ("duplicate-owner-lease", |s| {
            s["owners"][0]["id"] = s["leases"][0]["id"].clone()
        }),
        ("pin-reference", |s| {
            s["pins"][0]["modules"][0] = 999u64.into()
        }),
        ("pure-pin", |s| s["pure_captures"][0]["pin"] = 999u64.into()),
        ("pure-closure", |s| {
            s["pure_captures"][0]["closure"]
                .as_array_mut()
                .unwrap()
                .pop();
        }),
        ("phase-bound", |s| {
            s["operations"][0]["phase"] = "x".repeat(257).into()
        }),
        ("namespace", |s| s["source_namespace"] = 0u64.into()),
        ("tombstone-live-alias", |s| {
            let id = s["leases"][0]["id"].clone();
            s["disposed_sources"].as_array_mut().unwrap().push(id);
        }),
    ];
    for (name, mutate) in mutations {
        eprintln!("mutation {name}");
        refuse(*mutate, |_| {});
    }
}
#[test]
fn independent_authority_artifacts_cleanup_and_resource_alias_refusals() {
    refuse(|_| {}, |h| h.narrow = true);
    refuse(|_| {}, |h| h.deny = true);
    refuse(|_| {}, |h| h.deny_cleanup = true);
    refuse(
        |_| {},
        |h| {
            let old = h
                .materials
                .iter()
                .find_map(|(d, m)| match m {
                    Material::Guest(m, _) if m.id == "old-helper" => Some(*d),
                    _ => None,
                })
                .unwrap();
            h.materials.remove(&old);
        },
    );
    refuse(
        |s| {
            s["registry"]["grants"][0]["active"] = false.into();
        },
        |_| {},
    );
    refuse(
        |s| {
            s["registry"]["grants"][0]["requested"]["scope"]["Keys"]
                .as_array_mut()
                .unwrap()
                .push(serde_json::json!({"owner":"service", "key":"other"}));
        },
        |h| h.alias_resources = true,
    );
}
#[test]
fn historical_highwaters_holes_and_exhaustion_survive() {
    let mut f = fixture();
    let mut s = saved(&mut f);
    s.registry.next_operation = u64::MAX;
    s.registry.retention_counters = [u64::MAX; 3];
    let host = Host::approved(f.host.materials.clone(), &s);
    let mut k = Kernel::new(Limits::default()).unwrap();
    let r = k.restore_kernel(&encoded(&s), &host).unwrap();
    let before = k.lifecycle_state();
    assert_eq!(k.issue_operation(), Err(Error::Limit));
    assert_eq!(
        k.capture_pipeline(k_handle(&s, &r, "actor"), "resume", auth()),
        Err(Error::Limit)
    );
    assert_eq!(
        k.retain_owner(&[k_handle(&s, &r, "actor")]),
        Err(Error::Limit)
    );
    assert!(matches!(
        k.pin_modules(&[k_handle(&s, &r, "actor")]),
        Err(Error::Limit)
    ));
    assert_eq!(
        k.adopt_callback(r.callbacks().values().next().unwrap()),
        Err(Error::Limit)
    );
    assert_eq!(k.lifecycle_state(), before);
    assert_eq!(host.effects.calls.get(), 0);
}

#[test]
fn recomputed_hashes_cannot_authorize_forged_source_or_incomplete_routes() {
    refuse(
        |value| {
            let mut s: KernelSnapshot = serde_json::from_value(value.clone()).unwrap();
            let origin = s.callbacks[0].pipeline.origin.as_mut().unwrap();
            assert_eq!(
                rehash(
                    s.source_namespace,
                    &origin.source_pipeline.root,
                    &origin.source_pipeline.ceiling,
                    &origin.source_pipeline.routing
                ),
                origin.source_pipeline.hash
            );
            // This import never could have invoked the capture service.
            let slot = origin
                .source_pipeline
                .routing
                .slots
                .iter_mut()
                .find(|slot| slot.caller == origin.caller.module && slot.slot == origin.slot)
                .unwrap();
            slot.ceiling = None;
            let witness = &mut origin.source_pipeline;
            witness.hash = rehash(
                s.source_namespace,
                &witness.root,
                &witness.ceiling,
                &witness.routing,
            );
            *value = serde_json::to_value(s).unwrap();
        },
        |_| {},
    );
    refuse(
        |value| {
            let mut s: KernelSnapshot = serde_json::from_value(value.clone()).unwrap();
            let p = &mut s.leases[0].pipeline;
            assert_eq!(
                rehash(s.source_namespace, &p.root, &p.ceiling, &p.routing),
                p.hash
            );
            p.routing.routes.pop();
            p.hash = rehash(s.source_namespace, &p.root, &p.ceiling, &p.routing);
            *value = serde_json::to_value(s).unwrap();
        },
        |_| {},
    );
    refuse(
        |value| {
            let mut s: KernelSnapshot = serde_json::from_value(value.clone()).unwrap();
            let helper = module(&s, "helper");
            let unrelated = s
                .registry
                .modules
                .iter()
                .find(|m| m.manifest.id == "actor")
                .unwrap();
            let p = &mut s.leases[0].pipeline;
            assert_eq!(
                rehash(s.source_namespace, &p.root, &p.ceiling, &p.routing),
                p.hash
            );
            let chain = &mut p
                .routing
                .routes
                .iter_mut()
                .find(|r| r.module == helper)
                .unwrap()
                .chain;
            chain[1] = SavedBinding {
                module: unrelated.id,
                artifact: unrelated.artifact,
                endpoint: "run".into(),
            };
            p.hash = rehash(s.source_namespace, &p.root, &p.ceiling, &p.routing);
            *value = serde_json::to_value(s).unwrap();
        },
        |_| {},
    );
}

// Repair regressions: the host catalog is frozen from trusted local state
// before any attacker-controlled wire mutations and survives source disposal.
#[test]
fn complete_context_policy_refuses_forged_historical_and_enrolled_bindings() {
    let mut f = fixture();
    let baseline = encoded(&saved(&mut f));
    let catalog = f.host.materials.clone();
    let approved = baseline.operation_contexts().unwrap();
    drop(f);
    type Mutation = fn(&mut KernelSnapshot);
    let mutations: &[Mutation] = &[
        |s| {
            s.callbacks[0]
                .pipeline
                .origin
                .as_mut()
                .unwrap()
                .source
                .operation = 3
        },
        |s| {
            s.callbacks[0]
                .pipeline
                .origin
                .as_mut()
                .unwrap()
                .source
                .checkpoint = 999
        },
        |s| {
            s.callbacks[0]
                .pipeline
                .origin
                .as_mut()
                .unwrap()
                .source
                .phase = "unrelated".into()
        },
        |s| {
            s.callbacks[0]
                .pipeline
                .origin
                .as_mut()
                .unwrap()
                .source
                .cleanup
                .clear()
        },
        |s| {
            let source = &mut s.callbacks[0].pipeline.origin.as_mut().unwrap().source;
            source.operation = 1;
            source.pipeline = 1;
            s.disposed_sources = [1].into();
        },
        |s| s.operations[0].operation = 2,
        |s| s.operations[0].checkpoint = 999,
        |s| s.operations[0].phase = "forged".into(),
        |s| s.operations[0].cleanup.clear(),
        |s| {
            // Two forged copies cannot approve one another.
            s.operations[0].operation = 1;
            s.callbacks[0]
                .pipeline
                .origin
                .as_mut()
                .unwrap()
                .source
                .operation = 1;
        },
    ];
    for mutate in mutations {
        let mut s = baseline.clone();
        mutate(&mut s);
        let host = Host::with_contexts(catalog.clone(), &approved);
        let mut destination = fixture();
        let before = destination.kernel.lifecycle_state();
        assert!(destination
            .kernel
            .restore_kernel(&encoded(&s), &host)
            .is_err());
        assert_eq!(host.factories.get(), 0);
        assert_eq!(host.effects.calls.get(), 0);
        assert!(host.effects.callbacks.borrow().is_empty());
        assert_eq!(destination.kernel.lifecycle_state(), before);
        assert_eq!(
            destination
                .kernel
                .invoke_operation(&destination.operation, &bytes(5), auth())
                .unwrap(),
            bytes(116)
        );
    }
    let missing = Host::fresh(catalog.clone());
    let mut k = Kernel::new(Limits::default()).unwrap();
    assert!(k.restore_kernel(&baseline, &missing).is_err());
    assert_eq!(missing.factories.get(), 0);
    let default = DefaultContextPolicy(Host::fresh(catalog.clone()));
    assert!(k.restore_kernel(&baseline, &default).is_err());
    assert_eq!(default.0.factories.get(), 0);
    let host = Host::with_contexts(catalog, &approved);
    let restored = k.restore_kernel(&baseline, &host).unwrap();
    let callback = restored.callbacks().values().next().unwrap();
    let (_, lease) = k.adopt_callback(callback).unwrap();
    let provenance = k.callback_provenance(lease).unwrap().unwrap();
    assert_eq!(provenance.source.operation().serial(), 2);
    assert_eq!(provenance.source.checkpoint(), 7);
    assert_eq!(provenance.source.phase(), "captured");
    assert!(provenance
        .source
        .admits_cleanup(*restored.cleanup().values().next().unwrap()));
    assert_eq!(
        k.invoke_operation(&restored.operations()[0], &bytes(5), auth())
            .unwrap(),
        bytes(116)
    );
}

#[test]
fn independent_capacity_domains_capture_and_restore_at_exact_boundary() {
    let mut f = fixture_with_retention(RetentionLimits {
        leases: 1,
        owners: 3,
        references: 24,
        ..RetentionLimits::default()
    });
    let state = f.kernel.lifecycle_state();
    assert_eq!(state.owners, 3);
    assert_eq!(state.callbacks, 1);
    assert_eq!(state.references, 24);
    let s = encoded(&saved(&mut f));
    let host = Host::approved(f.host.materials.clone(), &s);
    drop(f);
    let mut k = Kernel::new(Limits::default()).unwrap();
    let restored = k.restore_kernel(&s, &host).unwrap();
    let callback = restored.callbacks().values().next().unwrap().clone();
    let pin = restored.pins().values().next().unwrap().clone();
    let again = callback.clone();
    let shared_pin = pin.clone();
    assert_eq!(k.lifecycle_state().references, 24);
    assert_eq!(k.lifecycle_state().owners, 3);
    drop((again, shared_pin));
    assert_eq!(k.lifecycle_state().references, 24);
    // Runtime pure replies acquire temporary pins. Release retained RAII
    // reservations after proving restore's exact persistent boundary.
    let operation = restored.operations()[0].clone();
    drop((callback, pin, restored));
    assert_eq!(
        k.invoke_operation(&operation, &bytes(5), auth()).unwrap(),
        bytes(116)
    );
    type Mutation = fn(&mut KernelSnapshot);
    let exceeded: &[Mutation] = &[
        |s| s.registry.retention_limits.references = 23,
        |s| s.registry.retention_limits.owners = 2,
        |s| {
            let mut lease = s.leases[0].clone();
            lease.id = s.registry.retention_counters[0];
            s.registry.retention_counters[0] += 1;
            s.leases.push(lease);
        },
        |s| {
            for id in 2..=4 {
                let mut c = s.callbacks[0].clone();
                c.id = id;
                s.callbacks.push(c);
            }
            s.registry.retention_counters[1] = 5;
        },
        |s| s.registry.retention_limits.nodes = 1,
        |s| s.registry.retention_limits.slots = 1,
    ];
    for mutate in exceeded {
        let mut invalid = s.clone();
        mutate(&mut invalid);
        let before = k.lifecycle_state();
        assert!(k.restore_kernel(&encoded(&invalid), &host).is_err());
        assert_eq!(k.lifecycle_state(), before);
    }
}

#[test]
fn malformed_structure_and_aggregate_bounds_refuse_before_factories() {
    let mut f = fixture();
    let baseline = encoded(&saved(&mut f));
    let catalog = f.host.materials.clone();
    drop(f);
    type Mutation = fn(&mut KernelSnapshot);
    let mutations: &[Mutation] = &[
        |s| s.leases[0].pipeline.root.module = 999,
        |s| s.registry.retention_limits.references = 1,
        |s| s.leases[0].pipeline.routing.routes[0].module = 999,
        |s| s.leases[0].pipeline.routing.routes[0].chain[0].artifact = [99; 32],
        |s| {
            let route = s.leases[0].pipeline.routing.routes[0].clone();
            s.leases[0].pipeline.routing.routes.push(route);
        },
        |s| s.leases[0].pipeline.routing.slots[0].target = 999,
        |s| s.leases[0].pipeline.routing.slots[0].slot = usize::MAX,
        |s| {
            s.leases[0]
                .pipeline
                .routing
                .slots
                .pop()
                .map(|_| ())
                .unwrap()
        },
        |s| s.owners[0].modules.push(999),
        |s| {
            let id = s.pins[0].modules[0];
            s.pins[0].modules.push(id);
        },
        |s| s.pure_captures[0].pin = 999,
        |s| s.pure_captures[0].provider = 999,
        |s| s.pure_captures[0].closure[0].resolution[0].module = 999,
        |s| s.pure_captures[0].routing.slots[0].slot = usize::MAX,
        |s| s.operations[0].pipeline = 999,
        |s| {
            s.callbacks[0]
                .pipeline
                .origin
                .as_mut()
                .unwrap()
                .service
                .module = 999
        },
        |s| {
            s.callbacks[0]
                .pipeline
                .origin
                .as_mut()
                .unwrap()
                .source_pipeline
                .root
                .module = 999
        },
        |s| {
            s.callbacks[0]
                .pipeline
                .origin
                .as_mut()
                .unwrap()
                .service_route[0]
                .module = 999
        },
        |s| s.callbacks[0].pipeline.origin.as_mut().unwrap().target = 999,
        |s| s.callbacks[0].pipeline.origin.as_mut().unwrap().slot = usize::MAX,
        |s| s.registry.modules[0].dependencies.push(999),
        |s| {
            s.registry.modules[0]
                .manifest
                .exports
                .get_mut("run")
                .unwrap()
                .major = 0
        },
        |s| {
            s.registry.modules[0]
                .manifest
                .extension_points
                .insert("missing".into());
        },
        |s| s.registry.retention_limits.references = usize::MAX,
        |s| s.registry.grants[0].slot = usize::MAX,
    ];
    for mutate in mutations {
        let host = Host::approved(catalog.clone(), &baseline);
        let mut s = baseline.clone();
        mutate(&mut s);
        let mut destination = fixture();
        let before = destination.kernel.lifecycle_state();
        assert!(destination
            .kernel
            .restore_kernel(&encoded(&s), &host)
            .is_err());
        assert_eq!(host.factories.get(), 0);
        assert_eq!(host.effects.calls.get(), 0);
        assert_eq!(destination.kernel.lifecycle_state(), before);
        let restored = destination.kernel.restore_kernel(&baseline, &host).unwrap();
        assert_eq!(host.factories.get(), baseline.registry.modules.len());
        assert_eq!(
            destination
                .kernel
                .invoke_operation(&restored.operations()[0], &bytes(5), auth())
                .unwrap(),
            bytes(116)
        );
    }
}

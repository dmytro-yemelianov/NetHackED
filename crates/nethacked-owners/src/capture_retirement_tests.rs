#![allow(dead_code, unused_imports)]
use crate as nethacked_owners;
include!("../../nethacked-owner-services/tests/support/persistence.rs");
#[test]
fn weak_scopes_behaviors_and_candidates_hold_allocations_then_account_on_success_and_unwind() {
    for unwind in [false, true] {
        let mut h = Harness::new(Limits::default(), KernelLimits::default(), "");
        let effects = Rc::new(Cell::new(0));
        let owner = h
            .host
            .enroll(
                &h.kernel,
                h.spec(1, vec![]),
                bytes(5),
                Backend::Wasm {
                    codec: Rc::new(Domain {
                        add: 1,
                        effects: effects.clone(),
                    }),
                    slot: 0,
                    binding: h.binding(),
                },
            )
            .unwrap();
        h.admit(&owner);
        h.refresh();
        let behavior = Rc::new(RefCell::new(None));
        let callback = Rc::new(RefCell::new(None));
        let candidate = Rc::new(RefCell::new(None));
        h.run(1, {
            let owner = owner.clone();
            let behavior = behavior.clone();
            let callback = callback.clone();
            let candidate = candidate.clone();
            let keys = h.keys.clone();
            move |view| {
                *behavior.borrow_mut() = Some(owner.retain_behavior("prepare", view).unwrap());
                *callback.borrow_mut() = Some(view.capture_callback(
                    "run",
                    Authority {
                        access: Access::Write,
                        scope: Scope::Keys(keys.clone()),
                    },
                )?);
                *candidate.borrow_mut() =
                    Some(owner.prepare("prepare", &[], vec![], view).unwrap());
                Ok(vec![])
            }
        });
        let retained = behavior.borrow_mut().take().unwrap();
        let callback = callback.borrow_mut().take().unwrap();
        let scoped = retained.bind_callback(&callback).unwrap();
        drop(retained);
        drop(callback);
        let candidate = candidate.borrow_mut().take().unwrap();
        let before_effects = effects.get();
        let before = h.kernel.lifecycle_state();
        let domain = Rc::new(nethacked_kernel::session::Domain::new());
        h.kernel.enroll_session(&domain.enrollment(1)).unwrap();
        h.host.enroll_session(&domain.enrollment(1)).unwrap();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = domain.capture().unwrap();
            drop(scoped);
            candidate.discard();
            assert_eq!(h.host.0.behaviors.get(), 2);
            assert_eq!(owner.metadata.retained_behaviors.get(), 1);
            assert_eq!(h.host.0.registry.borrow().pending, 1);
            assert_eq!(h.kernel.lifecycle_state(), before);
            assert_eq!(owner.inspect(), Err(Error::Busy));
            if unwind {
                panic!("capture unwinds after scope/candidate drop");
            }
        }));
        assert_eq!(result.is_err(), unwind);
        assert_eq!(h.host.retained_behaviors().unwrap(), 0);
        assert_eq!(h.host.pending().unwrap(), 0);
        assert_eq!(owner.metadata.pending.get(), 0);
        assert_eq!(h.host.0.registry.borrow().staging_bytes, 0);
        assert_eq!(effects.get(), before_effects);
        assert!(h.kernel.lifecycle_state().references < before.references);
    }
}

#[test]
fn owner_identity_and_counter_refusals_preserve_complete_capture() {
    fn snapshot(h: &mut Harness, owner: &Rc<Owner<u64>>) -> (Vec<u8>, u64) {
        let mut capture = h.host.capture_state().unwrap();
        capture.owner(owner, &NativeDomain).unwrap();
        let host = capture.finish().unwrap();
        let mut kernel = h
            .kernel
            .kernel_snapshot(&KernelEnrollment::default())
            .unwrap();
        let watermark = kernel.cleanup_high_water;
        // Process-wide cleanup allocation is independent of this host.
        kernel.cleanup_high_water = 0;
        (serde_json::to_vec(&(kernel, host)).unwrap(), watermark)
    }
    for exhausted in [false, true] {
        let mut h = Harness::new(Limits::default(), KernelLimits::default(), "");
        let owner = h
            .host
            .enroll(
                &h.kernel,
                h.spec(1, vec![]),
                7u64,
                Backend::Native(Rc::new(NativeDomain)),
            )
            .unwrap();
        h.host.0.registry.borrow_mut().next_owner = if exhausted {
            u64::MAX
        } else {
            owner.stamp().unwrap().owner.serial
        };
        let before = snapshot(&mut h, &owner);
        assert_eq!(
            h.host
                .enroll(
                    &h.kernel,
                    h.spec(1, vec![]),
                    9u64,
                    Backend::Native(Rc::new(NativeDomain))
                )
                .err(),
            Some(if exhausted {
                Error::Limit
            } else {
                Error::Invalid
            })
        );
        let after = snapshot(&mut h, &owner);
        assert!(after.1 >= before.1);
        assert_eq!(after.0, before.0);
        assert_eq!(owner.inspect().unwrap(), 7u64.to_le_bytes());
    }
}

#[path = "../../nethacked-inputs/tests/support/mod.rs"]
mod input_fixture;
mod support;
use nethacked_kernel::*;
use nethacked_timers::{Fault, Request, Service, Status, TargetValidator};
use std::{collections::BTreeSet, rc::Rc};
struct NoTargets;
impl TargetValidator for NoTargets {
    fn validate(&self, targets: &[nethacked_resources::Stamp]) -> Result<()> {
        if targets.is_empty() {
            Ok(())
        } else {
            Err(Error::NativeRefusal)
        }
    }
}
#[test]
fn real_recorded_input_prefix_survives_scheduling_and_callback_traps() {
    let mut h = input_fixture::Host::new();
    let s = Service::new(
        Default::default(),
        support::queue_key(),
        Rc::new(support::Clock::default()),
        Rc::new(NoTargets),
        true,
        vec![],
    )
    .unwrap();
    s.install(&mut h.k, "timers").unwrap();
    let mut m = input_fixture::manifest("timer-input-actor");
    m.dependencies.push(support::dep("timers", "queue"));
    let actor =
        h.k.register_guest(m, &input_fixture::forward("unreachable"))
            .unwrap();
    let auth = Authority {
        access: Access::Write,
        scope: Scope::Keys(BTreeSet::from([
            input_fixture::resource(),
            support::queue_key(),
        ])),
    };
    for slot in 0..2 {
        h.k.issue_grant(actor, slot, auth.clone()).unwrap();
    }
    let (op, lease) = h.k.capture_operation(actor, "step", auth.clone()).unwrap();
    let binding = h.k.operation_binding(op, lease, 4, "schedule").unwrap();
    let req = Request::Schedule {
        callback: "step".into(),
        payload: serde_json::to_vec(&input_fixture::sample(b"choice")).unwrap(),
        targets: vec![],
        deadline: vec![1],
        residency: vec![1],
        ceiling: auth.clone(),
    };
    let mut bytes = vec![32];
    bytes.extend(serde_json::to_vec(&req).unwrap());
    let before = h.s.inspect().unwrap();
    assert_eq!(
        h.k.invoke_operation(&binding, &bytes, auth),
        Err(Error::Trap)
    );
    assert_eq!(h.s.inspect().unwrap(), before);
    assert_eq!(s.state().unwrap().tasks.len(), 1);
    h.k.release_pipeline(lease).unwrap();
    let task = s
        .deliver_next(&mut h.k, &support::context(1, 1))
        .unwrap()
        .unwrap();
    assert_eq!(task.status, Status::Faulted(Fault::Invocation(Error::Trap)));
    let after = h.s.inspect().unwrap();
    assert_eq!(after.source, vec![20, 30, 40]);
    assert_eq!(after.records.len(), 1);
    assert_eq!(after.records[0].request, b"choice");
    assert_eq!(after.records[0].value, vec![10]);
    assert_eq!(Some(&after.records[0].operation), task.invocation.as_ref());
    assert_ne!(after.records[0].operation.operation(), op);
    assert_eq!(h.calls.get(), 1);
    let queue = s.state().unwrap();
    let lifecycle = h.k.lifecycle_state();
    assert_eq!(
        s.deliver_next(&mut h.k, &support::context(2, 1)).unwrap(),
        None
    );
    assert_eq!(s.state().unwrap(), queue);
    assert_eq!(h.k.lifecycle_state(), lifecycle);
    assert_eq!(h.s.inspect().unwrap(), after);
    assert_eq!(h.calls.get(), 1);
    s.dispose(&mut h.k, task.id).unwrap();
    assert_eq!(h.s.inspect().unwrap(), after);
}

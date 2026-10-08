#![allow(dead_code)]
use nethacked_kernel::*;
use nethacked_resources::{OwnerPolicy, Service as Resources, Stamp};
use nethacked_timers::{Limits as TimerLimits, Pending, Profile, Request, Service, TaskId};
use nethacked_workflows::CleanupParticipant;
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};
#[path = "../../nethacked-kernel/tests/callbacks_support/mod.rs"]
mod callback_fixture;
pub use callback_fixture::{cap, dep, key, manifest};
use nethacked_kernel as kernel_api;

pub fn queue_key() -> Resource {
    Resource {
        owner: "timers".into(),
        key: "queue".into(),
    }
}
pub fn auth() -> Authority {
    Authority {
        access: Access::Write,
        scope: Scope::Keys(BTreeSet::from([queue_key(), key("value")])),
    }
}
pub fn escape(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("\\{b:02x}")).collect()
}
pub fn abi(bytes: &[u8]) -> i32 {
    i32::from_le_bytes(bytes.try_into().unwrap())
}
pub fn context(now: u8, level: u8) -> Vec<u8> {
    vec![now, level]
}
#[derive(Default)]
pub struct Clock {
    pub reverse: bool,
    pub inactive: bool,
    pub forced: Cell<Option<TaskId>>,
}
impl Profile for Clock {
    fn validate(&self, deadline: &[u8], residency: &[u8]) -> Result<()> {
        if deadline.len() == 1 && residency.len() == 1 {
            Ok(())
        } else {
            Err(Error::NativeRefusal)
        }
    }
    fn ready(&self, p: &Pending, context: &[u8]) -> Result<bool> {
        if context.len() != 2 {
            return Err(Error::NativeRefusal);
        }
        Ok(p.deadline[0] <= context[0] && (self.inactive || p.residency[0] == context[1]))
    }
    fn select(&self, pending: &[Pending], context: &[u8]) -> Result<Option<TaskId>> {
        if let Some(id) = self.forced.get() {
            return Ok(Some(id));
        }
        let mut eligible = vec![];
        for p in pending {
            if self.ready(p, context)? {
                eligible.push(p);
            }
        }
        eligible.sort_by_key(|p| (p.deadline[0], p.id));
        Ok(if self.reverse {
            eligible.last()
        } else {
            eligible.first()
        }
        .map(|p| p.id))
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Seen {
    pub caller: ModuleHandle,
    pub binding: OperationBinding,
    pub payload: Vec<u8>,
}
/// Guest runs queue slot 0; callback runs effect slot 1 and returns its JSON outcome.
pub fn guest(callback_body: &str, tail: &str) -> Vec<u8> {
    let normal = r#"i32.const 1 local.get $p local.get $n call $forward"#;
    wat::parse_str(format!(r#"(module
      (import "kernel" "call" (func $call (param i32 i32 i32 i32 i32) (result i32)))
      (memory (export "memory") 1)
      (data (i32.const 8192) "{{\22readonly\22:[],\22writes\22:[],\22events\22:[]}}")
      (func (export "alloc") (param i32) (result i32) i32.const 0)
      (func $forward (param $s i32) (param $p i32) (param $n i32) (result i64) (local $r i32)
        local.get $s local.get $p local.get $n i32.const 32768 i32.const 16384 call $call local.set $r
        local.get $r i32.const 0 i32.lt_s if
          i32.const 32768 local.get $r i32.store i32.const 4 local.set $r end
        i64.const 140737488355328 local.get $r i64.extend_i32_u i64.or)
      (func (export "run") (param $p i32) (param $n i32) (result i64)
        i32.const 0 local.get $p local.get $n call $forward {tail})
      (func (export "callback") (param $p i32) (param $n i32) (result i64) (local $r i32)
        {}))"#, if callback_body.is_empty() { normal } else { callback_body })).unwrap()
}
pub struct Host {
    pub k: Kernel,
    pub s: Service,
    pub resources: Resources,
    pub stamp: Stamp,
    pub actor: ModuleHandle,
    pub effect: ModuleHandle,
    pub timer: ModuleHandle,
    pub seen: Rc<RefCell<Vec<Seen>>>,
    pub effect_reply: Rc<RefCell<Vec<u8>>>,
    pub hook: Rc<RefCell<Option<NativePort>>>,
    pub clock: Rc<Clock>,
}
impl Default for Host {
    fn default() -> Self {
        Self::new()
    }
}
impl Host {
    pub fn new() -> Self {
        Self::with(
            TimerLimits::default(),
            Rc::new(Clock::default()),
            "",
            "",
            vec![],
            true,
        )
    }
    pub fn with(
        limits: TimerLimits,
        clock: Rc<Clock>,
        body: &str,
        tail: &str,
        extra: Vec<Rc<dyn CleanupParticipant>>,
        validate_on_schedule: bool,
    ) -> Self {
        let mut k = Kernel::new(Limits::default()).unwrap();
        let seen = Rc::new(RefCell::new(vec![]));
        let effect_reply = Rc::new(RefCell::new(
            br#"{"kind":"Completed","result":[1]}"#.to_vec(),
        ));
        let hook = Rc::new(RefCell::new(None::<NativePort>));
        let seen_copy = seen.clone();
        let reply_copy = effect_reply.clone();
        let hook_copy = hook.clone();
        let mut m = manifest("effect", &["run"]);
        m.extension_points.insert("run".into());
        let effect = k
            .register_native(
                m,
                b"timer-effect-v1",
                BTreeMap::from([(
                    "run".into(),
                    Rc::new(move |bytes: &[u8], view: &CallerView| {
                        seen_copy.borrow_mut().push(Seen {
                            caller: view.caller().ok_or(Error::Authority)?,
                            binding: view.operation().ok_or(Error::Authority)?.clone(),
                            payload: bytes.to_vec(),
                        });
                        if let Some(hook) = hook_copy.borrow().as_ref() {
                            hook(bytes, view)?;
                        }
                        Ok(reply_copy.borrow().clone())
                    }) as NativePort,
                )]),
            )
            .unwrap();
        let resources =
            Resources::new(nethacked_resources::Limits::default(), Rc::new(|_| true)).unwrap();
        resources.install(&mut k, "resources").unwrap();
        resources
            .register_owner(
                "device",
                OwnerPolicy {
                    capability: cap("effect", "run"),
                    provider: effect,
                    artifact: k.artifact(effect).unwrap(),
                    validate: Rc::new(|_| true),
                },
            )
            .unwrap();
        let stamp = resources.put(key("value"), b"old".to_vec()).unwrap();
        resources
            .put(key("unrelated"), b"populated".to_vec())
            .unwrap();
        let mut participants: Vec<Rc<dyn CleanupParticipant>> = vec![Rc::new(resources.clone())];
        participants.extend(extra);
        let s = Service::new(
            limits,
            queue_key(),
            clock.clone(),
            Rc::new(resources.clone()),
            validate_on_schedule,
            participants,
        )
        .unwrap();
        let timer = s.install(&mut k, "timers").unwrap();
        let mut m = manifest("actor", &["run", "callback"]);
        m.exports.values_mut().for_each(|e| {
            e.max_request = 16384;
            e.max_reply = 16384;
        });
        m.dependencies = vec![
            dep("timers", "queue"),
            dep("effect", "run"),
            dep("resources", "prepare"),
            dep("resources", "commit"),
        ];
        m.extension_points.insert("callback".into());
        let actor = k.register_guest(m, &guest(body, tail)).unwrap();
        for slot in 0..4 {
            k.issue_grant(actor, slot, auth()).unwrap();
        }
        Self {
            k,
            s,
            resources,
            stamp,
            actor,
            effect,
            timer,
            seen,
            effect_reply,
            hook,
            clock,
        }
    }
    pub fn request(&self, payload: Vec<u8>, deadline: u8, level: u8) -> Request {
        Request::Schedule {
            callback: "callback".into(),
            payload,
            targets: vec![self.stamp.clone()],
            deadline: vec![deadline],
            residency: vec![level],
            ceiling: auth(),
        }
    }
    pub fn source(&mut self, actor: ModuleHandle) -> (PipelineLease, OperationBinding) {
        let (op, lease) = self.k.capture_operation(actor, "run", auth()).unwrap();
        let binding = self.k.operation_binding(op, lease, 7, "schedule").unwrap();
        (lease, binding)
    }
    pub fn invoke(&self, binding: &OperationBinding, request: &Request) -> Vec<u8> {
        self.k
            .invoke_operation(binding, &serde_json::to_vec(request).unwrap(), auth())
            .unwrap()
    }
    pub fn schedule(&mut self, payload: Vec<u8>, deadline: u8, level: u8) -> TaskId {
        let req = self.request(payload, deadline, level);
        let (lease, binding) = self.source(self.actor);
        let reply = self.invoke(&binding, &req);
        let id: TaskId = serde_json::from_slice(&reply).unwrap();
        assert_eq!(
            self.s.state().unwrap().tasks[&id].provenance.source,
            binding
        );
        self.k.release_pipeline(lease).unwrap();
        id
    }
    pub fn snapshot(
        &self,
    ) -> (
        nethacked_timers::State,
        LifecycleState,
        nethacked_resources::State,
        Vec<Seen>,
    ) {
        (
            self.s.state().unwrap(),
            self.k.lifecycle_state(),
            self.resources.state().unwrap(),
            self.seen.borrow().clone(),
        )
    }
    pub fn deliver(&mut self, now: u8, level: u8) -> nethacked_timers::Task {
        self.s
            .deliver_next(&mut self.k, &context(now, level))
            .unwrap()
            .unwrap()
    }
    pub fn extension(&mut self, id: &str, result: u8) -> ModuleHandle {
        let reply = format!(r#"{{"kind":"Completed","result":[{result}]}}"#);
        let code = wat::parse_str(format!(r#"(module
            (import "kernel" "call" (func $call (param i32 i32 i32 i32 i32) (result i32)))
            (memory (export "memory") 1)
            (data (i32.const 8192) "{}")
            (func (export "alloc") (param i32) (result i32) i32.const 0)
            (func (export "callback") (param $p i32) (param $n i32) (result i64)
                i32.const 0 local.get $p local.get $n i32.const 32768 i32.const 16384 call $call drop
                i64.const {}))"#, escape(reply.as_bytes()), (8192u64 << 32) | reply.len() as u64)).unwrap();
        let mut m = manifest(id, &["callback"]);
        m.dependencies = vec![dep("effect", "run")];
        m.exports.values_mut().for_each(|e| {
            e.max_request = 16384;
            e.max_reply = 16384;
        });
        m.extension = Some(Extension {
            target: cap("actor", "callback"),
            export: "callback".into(),
        });
        let handle = self.k.register_guest(m, &code).unwrap();
        self.k.issue_grant(handle, 0, auth()).unwrap();
        handle
    }
}

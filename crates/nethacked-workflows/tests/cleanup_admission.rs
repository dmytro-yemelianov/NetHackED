use nethacked_kernel::*;
use nethacked_workflows::{Error as WError, *};
use std::{
    cell::Cell,
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};

struct Participant(Option<CleanupIdentity>);
impl CleanupParticipant for Participant {
    fn identity(&self) -> nethacked_kernel::Result<Option<CleanupIdentity>> {
        Ok(self.0)
    }
    fn discard(&self, _: &OperationBinding) -> nethacked_kernel::Result<()> {
        panic!("refused invocation must not reach cleanup")
    }
}

#[test]
fn cleanup_enrollment_preserves_start_then_invocation_buffer_refusal() {
    for variant in 0..3 {
        let mut k = Kernel::new(nethacked_kernel::Limits {
            payload_bytes: 2,
            ..Default::default()
        })
        .unwrap();
        let manifest = Manifest {
            id: "tiny".into(),
            exports: BTreeMap::from([(
                "step".into(),
                Endpoint {
                    major: 1,
                    access: Access::Read,
                    max_request: 2,
                    max_reply: 2,
                },
            )]),
            dependencies: vec![],
            extension_points: BTreeSet::new(),
            extension: None,
        };
        let calls = Rc::new(Cell::new(0));
        let c = calls.clone();
        let port: NativePort = Rc::new(move |_: &[u8], _: &CallerView| {
            c.set(c.get() + 1);
            Ok(vec![])
        });
        let provider = k
            .register_native(manifest, b"tiny", BTreeMap::from([("step".into(), port)]))
            .unwrap();
        let w = Workflows::new(nethacked_workflows::Limits::default()).unwrap();
        let p: Rc<dyn CleanupParticipant> = Rc::new(Participant(if variant == 2 {
            Some(CleanupIdentity::issue().unwrap())
        } else {
            None
        }));
        let participants = if variant == 0 {
            vec![]
        } else {
            vec![p.clone(), p]
        };
        let id = w
            .start(
                &mut k,
                Program {
                    provider,
                    endpoint: "step".into(),
                    authority: Authority {
                        access: Access::Read,
                        scope: Scope::Keys(BTreeSet::new()),
                    },
                    initial: "abc".into(),
                    phases: ["abc".into()].into(),
                    allowed: BTreeSet::new(),
                },
                vec![],
                participants,
            )
            .unwrap();
        let before = w.inspect(id).unwrap();
        assert_eq!(
            w.step(&k, id, &[]),
            Err(WError::Kernel(nethacked_kernel::Error::Buffer))
        );
        assert_eq!(w.inspect(id).unwrap(), before);
        assert_eq!(calls.get(), 0);
    }
}

use super::*;
struct Payload {
    value: u8,
    host: Host,
    drops: Rc<Cell<usize>>,
    panic: bool,
}
impl Drop for Payload {
    fn drop(&mut self) {
        crate::probes::payload_drop();
        if self.value == 0 {
            assert!(!self.host.0.gate.get());
            let history = self.host.history().unwrap();
            assert_eq!(history.len(), 1);
            self.drops.set(self.drops.get() + 1);
            if self.panic {
                panic!("old-payload-after-committed-prefix");
            }
        }
    }
}
struct Domain;
impl Schema<Payload> for Domain {
    fn validate(&self, _: &Payload) -> Result<()> {
        crate::probes::owner();
        Ok(())
    }
    fn encode(&self, s: &Payload) -> Result<Vec<u8>> {
        crate::probes::codec();
        Ok(vec![s.value])
    }
    fn query(&self, s: &Payload, _: &str, _: &[u8], _: &[Observation]) -> Result<Vec<u8>> {
        self.encode(s)
    }
}
impl NativeBehavior<Payload> for Domain {
    fn prepare(
        &self,
        s: &Payload,
        _: &str,
        _: &[u8],
        _: &[Observation],
    ) -> Result<Proposal<Payload>> {
        crate::probes::owner();
        Ok(Proposal {
            next: Payload {
                value: 1,
                host: s.host.clone(),
                drops: s.drops.clone(),
                panic: s.panic,
            },
            events: vec![b"scalar-updated".to_vec()],
            reply: vec![1],
        })
    }
}
#[test]
fn old_payload_drops_after_opening_and_panic_quarantines_committed_receipts() {
    for panic in [false, true] {
        let m = Multi::new();
        let drops = Rc::new(Cell::new(0));
        let scalar = m
            .host
            .enroll(
                &m.kernel,
                m.spec(0, 5),
                Payload {
                    value: 0,
                    host: m.host.clone(),
                    drops: drops.clone(),
                    panic,
                },
                Backend::Native(Rc::new(Domain)),
            )
            .unwrap();
        let b = m.batch(
            vec![m.prepare(0, scalar.clone(), 1).erase()],
            vec![scalar.id()],
            vec![scalar.id()],
            Rc::new(Allow),
            vec![],
        );
        let during = Rc::new(Cell::new(0));
        let reached = during.clone();
        let seen = drops.clone();
        let batch = b.clone();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            m.run(10, move |v| {
                let seen = seen.clone();
                let reached = reached.clone();
                SWAP_PROBE.with(|p| {
                    *p.borrow_mut() = Some(Box::new(move || {
                        assert_eq!(seen.get(), 0);
                        reached.set(reached.get() + 1)
                    }))
                });
                let r = batch.commit(v).unwrap();
                Ok(r.reply)
            });
        }));
        SWAP_PROBE.with(|p| p.borrow_mut().take());
        assert_eq!(during.get(), 1);
        assert_eq!(drops.get(), 1);
        assert_eq!(crate::probes::counts(), [0; 5]);
        assert_eq!(result.is_err(), panic);
        if panic {
            assert_eq!(scalar.inspect(), Err(Error::Quarantined));
            assert_eq!(m.host.pending(), Err(Error::Quarantined));
            let prefix = m.host.quarantine_history().unwrap();
            assert_eq!(prefix.len(), 1);
            assert_eq!(scalar.slot.borrow().state.value, 1);
            assert_eq!(
                prefix[0].participants[0].next_digest,
                <[u8; 32]>::from(Sha256::digest([1]))
            );
            assert_eq!(prefix[0].epoch, 1);
            assert_eq!(prefix[0].events, vec![b"scalar-updated".to_vec()]);
            let batch = b.clone();
            m.run(10, move |v| {
                assert_eq!(batch.commit(v), Err(Error::Quarantined));
                Ok(vec![])
            });
        } else {
            assert_eq!(scalar.inspect().unwrap(), [1]);
            assert_eq!(m.host.history().unwrap().len(), 1);
        }
    }
}

mod support;
use nethacked_owners::*;
use std::{cell::Cell, rc::Rc};
use support::*;

#[derive(Clone)]
struct Equipment {
    slot: Option<OwnerId>,
}
struct EquipmentDomain {
    item: OwnerId,
}
impl Schema<Equipment> for EquipmentDomain {
    fn validate(&self, s: &Equipment) -> nethacked_owners::Result<()> {
        if s.slot.is_none() || s.slot == Some(self.item) {
            Ok(())
        } else {
            Err(Error::Invariant)
        }
    }
    fn encode(&self, s: &Equipment) -> nethacked_owners::Result<Vec<u8>> {
        Ok(vec![u8::from(s.slot.is_some())])
    }
    fn query(
        &self,
        s: &Equipment,
        _: &str,
        _: &[u8],
        _: &[Observation],
    ) -> nethacked_owners::Result<Vec<u8>> {
        Ok(u32::from(s.slot.is_some()).to_le_bytes().to_vec())
    }
}
impl NativeBehavior<Equipment> for EquipmentDomain {
    fn prepare(
        &self,
        _: &Equipment,
        _: &str,
        r: &[u8],
        _: &[Observation],
    ) -> nethacked_owners::Result<Proposal<Equipment>> {
        let slot = match r {
            [0] => None,
            [1] => Some(self.item),
            _ => return Err(Error::Invalid),
        };
        Ok(Proposal {
            next: Equipment { slot },
            events: vec![if slot.is_some() {
                b"equip".to_vec()
            } else {
                b"unequip".to_vec()
            }],
            reply: vec![],
        })
    }
}
#[derive(Clone)]
struct Item {
    active: bool,
    contribution: u32,
}
struct ItemDomain;
impl Schema<Item> for ItemDomain {
    fn validate(&self, s: &Item) -> nethacked_owners::Result<()> {
        if s.contribution <= 20 {
            Ok(())
        } else {
            Err(Error::Invariant)
        }
    }
    fn encode(&self, s: &Item) -> nethacked_owners::Result<Vec<u8>> {
        Ok([
            vec![u8::from(s.active)],
            s.contribution.to_le_bytes().to_vec(),
        ]
        .concat())
    }
    fn query(
        &self,
        s: &Item,
        _: &str,
        _: &[u8],
        _: &[Observation],
    ) -> nethacked_owners::Result<Vec<u8>> {
        Ok(if s.active { s.contribution } else { 0 }
            .to_le_bytes()
            .to_vec())
    }
}
impl NativeBehavior<Item> for ItemDomain {
    fn prepare(
        &self,
        _: &Item,
        _: &str,
        r: &[u8],
        _: &[Observation],
    ) -> nethacked_owners::Result<Proposal<Item>> {
        if r.len() != 2 || r[0] > 1 {
            return Err(Error::Invalid);
        }
        Ok(Proposal {
            next: Item {
                active: r[0] == 1,
                contribution: u32::from(r[1]),
            },
            events: vec![if r[0] == 1 {
                b"contribution".to_vec()
            } else {
                b"retire".to_vec()
            }],
            reply: vec![],
        })
    }
}
struct EquipmentInvariant {
    equipment: OwnerId,
    item: OwnerId,
    reached: Rc<Cell<usize>>,
}
impl Accounting for EquipmentInvariant {
    fn validate(&self, p: &[ProposalView], _: &[Observation]) -> nethacked_owners::Result<()> {
        self.reached.set(self.reached.get() + 1);
        if p.len() != 2 {
            return Err(Error::Invariant);
        }
        let equipment = p
            .iter()
            .find(|p| p.receipt.stamp.owner == self.equipment)
            .ok_or(Error::Invariant)?;
        let item = p
            .iter()
            .find(|p| p.receipt.stamp.owner == self.item)
            .ok_or(Error::Invariant)?;
        if equipment.proposed[0] != item.proposed[0] {
            return Err(Error::Invariant);
        }
        Ok(())
    }
}
struct PropertyCodec(Rc<Cell<usize>>);
impl Schema<u32> for PropertyCodec {
    fn validate(&self, s: &u32) -> nethacked_owners::Result<()> {
        ScalarCodec(self.0.clone()).validate(s)
    }
    fn encode(&self, s: &u32) -> nethacked_owners::Result<Vec<u8>> {
        Ok(s.to_le_bytes().to_vec())
    }
    fn query(
        &self,
        _: &u32,
        _: &str,
        _: &[u8],
        _: &[Observation],
    ) -> nethacked_owners::Result<Vec<u8>> {
        panic!("derived query must reach guest")
    }
}
impl WasmCodec<u32> for PropertyCodec {
    fn input(
        &self,
        _: &u32,
        _: &str,
        _: &[u8],
        o: &[Observation],
    ) -> nethacked_owners::Result<Vec<u8>> {
        self.0.set(self.0.get() + 1);
        if o.len() != 2 {
            return Err(Error::Invalid);
        }
        let equipped = u32::from_le_bytes(o[0].bytes().try_into().map_err(|_| Error::Invalid)?);
        let item = u32::from_le_bytes(o[1].bytes().try_into().map_err(|_| Error::Invalid)?);
        Ok([
            equipped
                .checked_mul(item)
                .ok_or(Error::Limit)?
                .to_le_bytes(),
            [0; 4],
            [0; 4],
            [0; 4],
        ]
        .concat())
    }
    fn decode(&self, b: &[u8]) -> nethacked_owners::Result<Proposal<u32>> {
        ScalarCodec(self.0.clone()).decode(b)
    }
    fn decode_query(&self, b: &[u8]) -> nethacked_owners::Result<Vec<u8>> {
        Ok(self.decode(b)?.reply)
    }
}
fn effective(
    h: &Harness,
    op: &nethacked_kernel::OperationBinding,
    e: Rc<Owner<Equipment>>,
    i: Rc<Owner<Item>>,
    p: Rc<Owner<u32>>,
    cache: Rc<QueryCache>,
    expected: (u32, bool),
) {
    let (value, hit) = expected;
    let observations = vec![observe(h, op, e), observe(h, op, i)];
    h.run(op, 2, &[], move |_, _, v| {
        let result = p
            .query_cached(&cache, "derived", &[], &observations, v)
            .unwrap();
        assert_eq!(result.hit, hit);
        assert_eq!(result.observation.bytes(), value.to_le_bytes());
        Ok(vec![])
    });
}
fn change(
    h: &Harness,
    op: &nethacked_kernel::OperationBinding,
    e: Rc<Owner<Equipment>>,
    i: Rc<Owner<Item>>,
    equipped: bool,
    contribution: u8,
    checks: Rc<Cell<usize>>,
) {
    let host = h.host.clone();
    h.run(op, 1, &[], move |_, _, v| {
        let equipment = e
            .prepare("prepare", &[u8::from(equipped)], vec![], v)
            .unwrap();
        let item = i
            .prepare("prepare", &[u8::from(equipped), contribution], vec![], v)
            .unwrap();
        let writes = [e.id(), i.id()];
        host.batch(
            vec![equipment.erase(), item.erase()],
            vec![],
            &writes,
            vec![e.id(), i.id()],
            [7; 32],
            Rc::new(EquipmentInvariant {
                equipment: e.id(),
                item: i.id(),
                reached: checks.clone(),
            }),
            cap("owners", "effect"),
            v,
        )
        .unwrap()
        .commit(v)
        .unwrap();
        Ok(vec![])
    });
}
#[test]
fn equipment_item_equip_retire_and_dynamic_derived_cache_are_coherent_and_local() {
    let mut h = Harness::new();
    let d = device(&mut h);
    let unrelated = scalar(&mut h, &d, Rc::new(Cell::new(0)));
    let item = h
        .host
        .enroll(
            &h.kernel,
            h.spec(4, vec![]),
            Item {
                active: false,
                contribution: 4,
            },
            Backend::Native(Rc::new(ItemDomain)),
        )
        .unwrap();
    h.admit(&item);
    let equipment = h
        .host
        .enroll(
            &h.kernel,
            h.spec(5, vec![]),
            Equipment { slot: None },
            Backend::Native(Rc::new(EquipmentDomain { item: item.id() })),
        )
        .unwrap();
    h.admit(&equipment);
    let reached = Rc::new(Cell::new(0));
    let codec = Rc::new(PropertyCodec(reached.clone()));
    let property = h
        .host
        .enroll(
            &h.kernel,
            h.spec(6, vec![dep(&equipment, "query"), dep(&item, "query")]),
            0u32,
            Backend::Wasm {
                codec: codec.clone(),
                slot: 0,
                binding: h.binding(),
            },
        )
        .unwrap();
    property
        .admit_compatible_wasm(&h.kernel, [6; 32], codec, 0, h.binding())
        .unwrap();
    h.admit(&property);
    let policy = QueryPolicy::Deterministic {
        profile: [7; 32],
        input_policy: [0; 32],
    };
    property.set_query_policy(policy.clone()).unwrap();
    unrelated.set_query_policy(policy).unwrap();
    let cache = property.query_cache().unwrap();
    let other_cache = unrelated.query_cache().unwrap();
    let op = h.operation();
    let checks = Rc::new(Cell::new(0));
    effective(
        &h,
        &op,
        equipment.clone(),
        item.clone(),
        property.clone(),
        cache.clone(),
        (1, false),
    );
    effective(
        &h,
        &op,
        equipment.clone(),
        item.clone(),
        property.clone(),
        cache.clone(),
        (1, true),
    );
    let device_observation = observe(&h, &op, d.clone());
    let other = unrelated.clone();
    let other_result = other_cache.clone();
    h.run(&op, 2, &[], move |_, _, v| {
        assert!(
            !other
                .query_cached(
                    &other_result,
                    "derived",
                    &[],
                    std::slice::from_ref(&device_observation),
                    v
                )
                .unwrap()
                .hit
        );
        Ok(vec![])
    });
    change(
        &h,
        &op,
        equipment.clone(),
        item.clone(),
        true,
        4,
        checks.clone(),
    );
    effective(
        &h,
        &op,
        equipment.clone(),
        item.clone(),
        property.clone(),
        cache.clone(),
        (5, false),
    );
    effective(
        &h,
        &op,
        equipment.clone(),
        item.clone(),
        property.clone(),
        cache.clone(),
        (5, true),
    );
    let device_observation = observe(&h, &op, d.clone());
    let other = unrelated.clone();
    let other_result = other_cache.clone();
    h.run(&op, 2, &[], move |_, _, v| {
        assert!(
            other
                .query_cached(
                    &other_result,
                    "derived",
                    &[],
                    std::slice::from_ref(&device_observation),
                    v
                )
                .unwrap()
                .hit
        );
        Ok(vec![])
    });
    change(
        &h,
        &op,
        equipment.clone(),
        item.clone(),
        true,
        8,
        checks.clone(),
    );
    effective(
        &h,
        &op,
        equipment.clone(),
        item.clone(),
        property.clone(),
        cache.clone(),
        (9, false),
    );
    let (_, binding) = h.replace();
    property
        .admit_compatible_wasm(
            &h.kernel,
            [6; 32],
            Rc::new(PropertyCodec(reached.clone())),
            0,
            binding,
        )
        .unwrap();
    let fresh = h.operation();
    effective(
        &h,
        &fresh,
        equipment.clone(),
        item.clone(),
        property.clone(),
        cache.clone(),
        (18, false),
    );
    effective(
        &h,
        &fresh,
        equipment.clone(),
        item.clone(),
        property.clone(),
        cache.clone(),
        (18, true),
    );
    change(
        &h,
        &fresh,
        equipment.clone(),
        item.clone(),
        false,
        8,
        checks.clone(),
    );
    effective(
        &h,
        &fresh,
        equipment.clone(),
        item.clone(),
        property.clone(),
        cache.clone(),
        (2, false),
    );
    assert_eq!(equipment.inspect().unwrap(), [0]);
    assert_eq!(item.inspect().unwrap(), [0, 8, 0, 0, 0]);
    assert_eq!(reached.get(), 5);
    assert!(checks.get() >= 3);
    assert_eq!(h.host.history().unwrap().len(), 3);
    let observations = vec![
        observe(&h, &fresh, equipment.clone()),
        observe(&h, &fresh, item.clone()),
    ];
    let target = property.clone();
    let cached = cache.clone();
    let host = h.host.clone();
    h.run(&fresh, 2, &[], move |_, _, v| {
        let gate = host.close_visibility().unwrap();
        assert!(matches!(
            target.query_cached(&cached, "derived", &[], &observations, v),
            Err(Error::Busy)
        ));
        drop(gate);
        Ok(vec![])
    });
    h.kernel.release_pipeline(op.pipeline()).unwrap();
    h.kernel.release_pipeline(fresh.pipeline()).unwrap();
    drop(cache);
    drop(other_cache);
    drop(property);
    drop(equipment);
    drop(item);
    drop(unrelated);
    drop(d);
    assert_eq!(h.kernel.lifecycle_state().references, 0);
}

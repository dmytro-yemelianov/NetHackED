use super::*;
#[derive(Debug)]
pub(super) struct Stock {
    pub qty: u8,
    pub capacity: u8,
}
pub(super) struct StockDomain;
impl Schema<Stock> for StockDomain {
    fn validate(&self, s: &Stock) -> Result<()> {
        crate::probes::owner();
        if s.qty <= s.capacity {
            Ok(())
        } else {
            Err(Error::Invariant)
        }
    }
    fn encode(&self, s: &Stock) -> Result<Vec<u8>> {
        crate::probes::codec();
        Ok(vec![s.qty, s.capacity])
    }
    fn query(&self, s: &Stock, _: &str, _: &[u8], _: &[Observation]) -> Result<Vec<u8>> {
        self.encode(s)
    }
}
impl NativeBehavior<Stock> for StockDomain {
    fn prepare(
        &self,
        s: &Stock,
        _: &str,
        req: &[u8],
        _: &[Observation],
    ) -> Result<Proposal<Stock>> {
        crate::probes::owner();
        let qty = if req == [0] {
            s.qty.checked_sub(1)
        } else {
            s.qty.checked_add(1)
        }
        .ok_or(Error::Invariant)?;
        Ok(Proposal {
            next: Stock {
                qty,
                capacity: s.capacity,
            },
            events: vec![vec![b's', qty]],
            reply: vec![qty],
        })
    }
}
#[derive(Debug)]
pub(super) struct Item {
    pub place: u8,
    pub live: bool,
}
pub(super) struct ItemDomain;
impl Schema<Item> for ItemDomain {
    fn validate(&self, s: &Item) -> Result<()> {
        crate::probes::owner();
        if s.place <= 1 {
            Ok(())
        } else {
            Err(Error::Invariant)
        }
    }
    fn encode(&self, s: &Item) -> Result<Vec<u8>> {
        crate::probes::codec();
        Ok(vec![s.place, u8::from(s.live)])
    }
    fn query(&self, s: &Item, _: &str, _: &[u8], _: &[Observation]) -> Result<Vec<u8>> {
        self.encode(s)
    }
}
impl NativeBehavior<Item> for ItemDomain {
    fn prepare(&self, s: &Item, _: &str, req: &[u8], _: &[Observation]) -> Result<Proposal<Item>> {
        crate::probes::owner();
        let next = if req == [2] {
            Item {
                place: s.place,
                live: false,
            }
        } else {
            Item {
                place: 1,
                live: s.live,
            }
        };
        Ok(Proposal {
            next,
            events: vec![vec![b'i', req[0]]],
            reply: vec![req[0]],
        })
    }
}
#[derive(Debug)]
pub(super) struct Debt {
    pub quantity: u8,
    pub consumed: u8,
    pub terms: u8,
    pub creditor: u8,
    pub liability: u8,
}
pub(super) struct DebtDomain;
impl Schema<Debt> for DebtDomain {
    fn validate(&self, s: &Debt) -> Result<()> {
        crate::probes::owner();
        if s.quantity.checked_add(s.consumed) == Some(1) {
            Ok(())
        } else {
            Err(Error::Invariant)
        }
    }
    fn encode(&self, s: &Debt) -> Result<Vec<u8>> {
        crate::probes::codec();
        Ok(vec![
            s.quantity,
            s.consumed,
            s.terms,
            s.creditor,
            s.liability,
        ])
    }
    fn query(&self, s: &Debt, _: &str, _: &[u8], _: &[Observation]) -> Result<Vec<u8>> {
        self.encode(s)
    }
}
impl NativeBehavior<Debt> for DebtDomain {
    fn prepare(&self, s: &Debt, _: &str, _: &[u8], _: &[Observation]) -> Result<Proposal<Debt>> {
        crate::probes::owner();
        Ok(Proposal {
            next: Debt {
                quantity: 0,
                consumed: 1,
                terms: s.terms,
                creditor: s.creditor,
                liability: s.liability,
            },
            events: vec![b"debt-retired".to_vec()],
            reply: vec![s.liability],
        })
    }
}
pub(super) struct Allow;
impl Accounting for Allow {
    fn validate(&self, _: &[ProposalView], _: &[Observation]) -> Result<()> {
        crate::probes::owner();
        Ok(())
    }
}
pub(super) struct Transfer {
    pub source: OwnerId,
    pub dest: OwnerId,
    pub item: OwnerId,
    pub reached: Rc<Cell<usize>>,
}
impl Accounting for Transfer {
    fn validate(&self, p: &[ProposalView], _: &[Observation]) -> Result<()> {
        crate::probes::owner();
        self.reached.set(self.reached.get() + 1);
        let get = |id| {
            p.iter()
                .find(|p| p.receipt.stamp.owner == id)
                .ok_or(Error::Invariant)
        };
        let s = get(self.source)?;
        let d = get(self.dest)?;
        let i = get(self.item)?;
        if p.len() != 3
            || s.before[0] != 1
            || d.before[0] != 0
            || s.proposed[0] != 0
            || d.proposed[0] != 1
            || i.before != [0, 1]
            || i.proposed != [1, 1]
        {
            return Err(Error::Invariant);
        }
        Ok(())
    }
}
pub(super) struct Retirement {
    pub item: OwnerId,
    pub debt: OwnerId,
    pub reached: Rc<Cell<usize>>,
}
impl Accounting for Retirement {
    fn validate(&self, p: &[ProposalView], _: &[Observation]) -> Result<()> {
        crate::probes::owner();
        self.reached.set(self.reached.get() + 1);
        let item = p
            .iter()
            .find(|p| p.receipt.stamp.owner == self.item)
            .ok_or(Error::Invariant)?;
        let debt = p
            .iter()
            .find(|p| p.receipt.stamp.owner == self.debt)
            .ok_or(Error::Invariant)?;
        if p.len() != 2
            || item.proposed != [1, 0]
            || debt.before[..2] != [1, 0]
            || debt.proposed[..2] != [0, 1]
            || debt.before[2..] != debt.proposed[2..]
        {
            return Err(Error::Invariant);
        }
        Ok(())
    }
}

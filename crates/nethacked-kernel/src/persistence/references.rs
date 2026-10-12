//! Typed transport inspection. Count each encoded link occurrence, including
//! repeated links to shared allocations; row definitions and counters are not
//! links. No allocation, resolution, or execution occurs here.
use super::*;

pub struct ReferenceCounter(Option<usize>);
impl Default for ReferenceCounter {
    fn default() -> Self {
        Self::new()
    }
}
impl ReferenceCounter {
    pub fn new() -> Self {
        Self(Some(0))
    }
    pub fn add(&mut self, count: usize) {
        self.0 = self.0.and_then(|n| n.checked_add(count));
    }
    pub fn total(self) -> Option<usize> {
        self.0
    }
    pub fn authority(&mut self, a: &Authority) {
        if let Scope::Keys(keys) = &a.scope {
            self.add(keys.len());
        }
    }
    pub fn operation(&mut self, o: &SavedOperation) {
        self.add(2);
        self.add(o.cleanup.len());
    }
    fn routing(&mut self, r: &SavedRouting) {
        for route in &r.routes {
            self.add(1);
            self.add(route.chain.len());
        }
        for slot in &r.slots {
            self.add(2);
            if let Some(a) = &slot.ceiling {
                self.authority(a);
            }
        }
    }
    fn witness(&mut self, w: &SavedWitness) {
        self.add(1);
        self.authority(&w.ceiling);
        self.routing(&w.routing);
    }
    fn origin(&mut self, o: &SavedOrigin) {
        self.add(3);
        self.authority(&o.ceiling);
        self.operation(&o.source);
        self.add(o.service_route.len());
        self.witness(&o.source_pipeline);
    }
    fn pipeline(&mut self, p: &SavedPipeline) {
        self.add(1);
        self.authority(&p.ceiling);
        self.routing(&p.routing);
        if let Some(o) = &p.origin {
            self.origin(o);
        }
    }
    pub fn resource_context(&mut self, c: &ResourceContext) {
        self.add(2);
        if let Some(o) = &c.operation {
            self.operation(o);
        }
        for p in &c.permits {
            self.add(1);
            self.add(p.resolution.len());
        }
    }
}
impl KernelSnapshot {
    pub fn count_references(&self, n: &mut ReferenceCounter) {
        for m in &self.registry.modules {
            n.add(1); /* executable artifact */
            n.add(m.dependencies.len());
            n.add(usize::from(m.target.is_some()));
        }
        for g in &self.registry.grants {
            n.add(2);
            n.authority(&g.requested);
        }
        for _ in &self.registry.dispatch {
            n.add(2);
        }
        for l in &self.leases {
            n.pipeline(&l.pipeline);
        }
        for r in self.owners.iter().chain(&self.pins) {
            n.add(r.modules.len());
        }
        for c in &self.callbacks {
            n.pipeline(&c.pipeline);
        }
        for o in &self.operations {
            n.operation(o);
        }
        for c in &self.pure_captures {
            n.add(2);
            n.authority(&c.ceiling);
            n.routing(&c.routing);
            for p in &c.closure {
                n.add(p.resolution.len());
                n.authority(&p.ceiling);
            }
        }
        n.add(self.service_cleanup.len());
        n.add(self.service_resources.len());
        n.add(self.disposed_sources.len());
    }
}

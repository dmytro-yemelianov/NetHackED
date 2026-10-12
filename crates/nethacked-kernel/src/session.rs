//! Serial exclusion shared by enrolled kernel, owner and service allocations.
//! Tokens are host-only, never save data. Unenrolled APIs retain their behavior.
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

trait CaptureRoot {
    fn live(&self) -> bool;
    fn hold(&self) -> Option<Box<dyn Any>>;
}
struct WeakRoot<T: ?Sized>(Weak<T>);
impl<T: ?Sized + 'static> CaptureRoot for WeakRoot<T> {
    fn live(&self) -> bool {
        self.0.strong_count() != 0
    }
    fn hold(&self) -> Option<Box<dyn Any>> {
        self.0.upgrade().map(|root| Box::new(root) as Box<dyn Any>)
    }
}
#[derive(Default)]
struct CaptureRoots(RefCell<Vec<Box<dyn CaptureRoot>>>);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Busy,
    Retired,
    Foreign,
    Exhausted,
    FailStop,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Idle,
    Runtime,
    Capture,
    Exclusive,
}
pub struct Domain {
    pub occupied: Cell<bool>,
    mode: Cell<Mode>,
    depth: Cell<usize>,
    epoch: Cell<u64>,
    generation: Cell<u64>,
    pub failed: Cell<bool>,
    roots: RefCell<Vec<Weak<CaptureRoots>>>,
    enrolled_retained: RefCell<Vec<Box<dyn Any>>>,
}
impl Default for Domain {
    fn default() -> Self {
        Self::new()
    }
}
impl Domain {
    pub fn new() -> Self {
        Self {
            occupied: Cell::new(false),
            mode: Cell::new(Mode::Idle),
            depth: Cell::new(0),
            epoch: Cell::new(1),
            generation: Cell::new(1),
            failed: Cell::new(false),
            roots: RefCell::new(Vec::new()),
            enrolled_retained: RefCell::new(Vec::new()),
        }
    }
    pub fn epoch(&self) -> u64 {
        self.epoch.get()
    }
    pub fn generation(&self) -> u64 {
        self.generation.get()
    }
    pub fn fail(&self) {
        self.failed.set(true);
    }
    pub fn failed(&self) -> bool {
        self.failed.get()
    }
    pub fn advance_epoch(&self) -> Result<u64, Error> {
        let next = self.epoch.get().checked_add(1).ok_or(Error::Exhausted)?;
        self.epoch.set(next);
        Ok(next)
    }
    pub fn exchange(&self, next: u64) {
        self.generation.set(next);
    }
    fn check(&self) -> Result<(), Error> {
        if self.failed.get() {
            Err(Error::FailStop)
        } else {
            Ok(())
        }
    }
    pub fn exclusive(&self) -> Result<Guard<'_>, Error> {
        if self.mode.get() != Mode::Idle {
            return Err(Error::Busy);
        }
        self.check()?;
        self.occupied.set(true);
        self.mode.set(Mode::Exclusive);
        self.depth.set(1);
        Ok(Guard {
            domain: self,
            retained: Vec::new(),
        })
    }
    pub fn capture(&self) -> Result<Guard<'_>, Error> {
        let mut guard = self.exclusive()?;
        self.mode.set(Mode::Capture);
        let mut roots = self.roots.borrow_mut();
        roots.retain(|r| r.strong_count() != 0);
        for roots in roots.iter().filter_map(Weak::upgrade) {
            let mut entries = roots.0.borrow_mut();
            entries.retain(|r| r.live());
            guard
                .retained
                .extend(entries.iter().filter_map(|r| r.hold()));
        }
        Ok(guard)
    }
    pub fn runtime(&self) -> Result<Guard<'_>, Error> {
        if self.mode.get() != Mode::Idle {
            return Err(Error::Busy);
        }
        self.enter()
    }
    fn enter(&self) -> Result<Guard<'_>, Error> {
        self.check()?;
        if matches!(self.mode.get(), Mode::Exclusive | Mode::Capture) {
            return Err(Error::Busy);
        }
        let next = self.depth.get().checked_add(1).ok_or(Error::Exhausted)?;
        self.occupied.set(true);
        self.mode.set(Mode::Runtime);
        self.depth.set(next);
        Ok(Guard {
            domain: self,
            retained: Vec::new(),
        })
    }
    pub fn enrollment(self: &Rc<Self>, generation: u64) -> Enrollment {
        Enrollment {
            domain: self.clone(),
            generation,
        }
    }
}
pub struct Guard<'a> {
    domain: &'a Domain,
    retained: Vec<Box<dyn Any>>,
}
impl Drop for Guard<'_> {
    fn drop(&mut self) {
        if self.domain.mode.get() == Mode::Capture {
            self.retained
                .append(&mut self.domain.enrolled_retained.borrow_mut());
        }
        let next = self.domain.depth.get() - 1;
        self.domain.depth.set(next);
        if next == 0 {
            self.domain.occupied.set(false);
            self.domain.mode.set(Mode::Idle);
        }
        // Release only after the complete encoder and its local gates/borrows
        // have finished. Drop accounting can now enter the domain normally.
        // Contain destructor panics even when releasing during encoder unwind.
        for root in self.retained.drain(..) {
            if let Err(payload) =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| drop(root)))
            {
                self.domain.fail();
                std::mem::forget(payload);
            }
        }
    }
}
#[derive(Clone)]
pub struct Enrollment {
    domain: Rc<Domain>,
    generation: u64,
}
impl Enrollment {
    /// Quarantine the shared destination after a mutating boundary fails.
    pub fn fail(&self) {
        self.domain.fail();
    }

    fn check(&self) -> Result<(), Error> {
        self.domain.check()?;
        if self.generation != self.domain.generation.get() {
            return Err(Error::Retired);
        }
        Ok(())
    }
    pub fn enter(&self) -> Result<Guard<'_>, Error> {
        self.check()?;
        self.domain.enter()
    }
    pub fn changed(&self) -> Result<(), Error> {
        self.preflight_change()?;
        self.domain.advance_epoch().map(|_| ())
    }
    pub fn preflight_change(&self) -> Result<(), Error> {
        self.check()?;
        if self.domain.mode.get() != Mode::Runtime {
            return Err(Error::Busy);
        }
        self.domain
            .epoch
            .get()
            .checked_add(1)
            .ok_or(Error::Exhausted)
            .map(|_| ())
    }
    fn same(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.domain, &other.domain) && self.generation == other.generation
    }
}
/// One immutable enrollment beside private state, shared by every alias.
#[derive(Default)]
pub struct Participant(RefCell<Option<Enrollment>>, Rc<CaptureRoots>);
impl Participant {
    /// Weak bookkeeping in ordinary execution; actual allocations are held
    /// through the entire enrolled capture, including all payload encoders.
    pub fn retain_during_capture<T: ?Sized + 'static>(&self, root: &Rc<T>) {
        let mut roots = self.1 .0.borrow_mut();
        roots.retain(|r| r.live());
        roots.push(Box::new(WeakRoot(Rc::downgrade(root))));
        if let Some(token) = self.0.borrow().as_ref() {
            if token.domain.mode.get() == Mode::Capture {
                token
                    .domain
                    .enrolled_retained
                    .borrow_mut()
                    .push(Box::new(root.clone()));
            }
        }
    }
    pub fn enrollment(&self) -> Result<Option<Enrollment>, Error> {
        self.0
            .try_borrow()
            .map(|t| t.clone())
            .map_err(|_| Error::Busy)
    }
    pub fn enroll(&self, token: &Enrollment) -> Result<(), Error> {
        let mut current = self.0.try_borrow_mut().map_err(|_| Error::Busy)?;
        if let Some(current) = current.as_ref() {
            if !current.same(token) {
                return Err(Error::Foreign);
            }
            return Ok(());
        }
        let mut roots = token.domain.roots.borrow_mut();
        roots.retain(|r| r.strong_count() != 0);
        roots.push(Rc::downgrade(&self.1));
        // Initial enrollment can itself occur inside HostSlot::capture. Protect
        // existing aliases for the remainder of that same encoder too.
        if token.domain.mode.get() == Mode::Capture {
            token
                .domain
                .enrolled_retained
                .borrow_mut()
                .extend(self.1 .0.borrow().iter().filter_map(|r| r.hold()));
        }
        *current = Some(token.clone());
        Ok(())
    }
    pub fn check_live(&self) -> Result<(), Error> {
        if let Some(t) = self.0.try_borrow().map_err(|_| Error::Busy)?.as_ref() {
            t.check()?;
        }
        Ok(())
    }
    pub fn capture_ready(&self) -> Result<(), Error> {
        if let Some(t) = self.0.try_borrow().map_err(|_| Error::Busy)?.as_ref() {
            t.check()?;
            if matches!(t.domain.mode.get(), Mode::Runtime | Mode::Exclusive) {
                return Err(Error::Busy);
            }
        }
        Ok(())
    }
    pub fn enter(&self) -> Result<ParticipantGuard, Error> {
        let token = self.0.try_borrow().map_err(|_| Error::Busy)?.clone();
        if let Some(t) = &token {
            t.check()?;
            let guard = t.domain.enter()?;
            std::mem::forget(guard);
        }
        Ok(ParticipantGuard(token))
    }
    pub fn changed(&self) -> Result<(), Error> {
        if let Some(t) = self.0.borrow().as_ref() {
            t.changed()?;
        }
        Ok(())
    }
    pub fn preflight_change(&self) -> Result<(), Error> {
        if let Some(t) = self.0.try_borrow().map_err(|_| Error::Busy)?.as_ref() {
            t.preflight_change()?;
        }
        Ok(())
    }
}
pub struct ParticipantGuard(Option<Enrollment>);
impl Drop for ParticipantGuard {
    fn drop(&mut self) {
        if let Some(t) = &self.0 {
            drop(Guard {
                domain: &t.domain,
                retained: Vec::new(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct DropWitness(Rc<Domain>, Rc<Cell<usize>>, bool);
    impl Drop for DropWitness {
        fn drop(&mut self) {
            let _guard = self.0.runtime().expect("retirement must follow exclusion");
            self.1.set(self.1.get() + 1);
            if self.2 {
                panic!("retirement panic");
            }
        }
    }
    #[test]
    fn enrollment_inside_capture_retains_roots_and_contains_retirement_panic() {
        for panic in [false, true] {
            let domain = Rc::new(Domain::new());
            let participant = Participant::default();
            let drops = Rc::new(Cell::new(0));
            let root = Rc::new(DropWitness(domain.clone(), drops.clone(), panic));
            participant.retain_during_capture(&root);
            let guard = domain.capture().unwrap();
            participant.enroll(&domain.enrollment(1)).unwrap();
            drop(root);
            assert_eq!(drops.get(), 0);
            assert!(matches!(participant.enter(), Err(Error::Busy)));
            drop(guard);
            assert_eq!(drops.get(), 1);
            assert_eq!(domain.failed(), panic);
        }
    }
    #[test]
    fn ordinary_unenrolled_roots_release_immediately_and_exhaustion_is_atomic() {
        let domain = Rc::new(Domain::new());
        let participant = Participant::default();
        let drops = Rc::new(Cell::new(0));
        let root = Rc::new(DropWitness(domain.clone(), drops.clone(), false));
        participant.retain_during_capture(&root);
        drop(root);
        assert_eq!(drops.get(), 1);
        participant.enroll(&domain.enrollment(1)).unwrap();
        domain.epoch.set(u64::MAX);
        let _guard = participant.enter().unwrap();
        assert_eq!(participant.preflight_change(), Err(Error::Exhausted));
        assert_eq!(participant.changed(), Err(Error::Exhausted));
        assert_eq!(domain.epoch(), u64::MAX);
    }
}

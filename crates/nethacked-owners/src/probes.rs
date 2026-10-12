//! Test-only allocator instrumentation; never linked into production builds.
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};
thread_local! {
    static ACTIVE: Cell<bool> = const { Cell::new(false) };
    static COUNTS: Cell<[usize;5]> = const { Cell::new([0;5]) };
}
struct Meter;
#[global_allocator]
static ALLOCATOR: Meter = Meter;
unsafe impl GlobalAlloc for Meter {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        mark(0);
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        mark(1);
        unsafe { System.dealloc(p, layout) }
    }
    unsafe fn realloc(&self, p: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        mark(0);
        mark(1);
        unsafe { System.realloc(p, layout, size) }
    }
}
fn mark(index: usize) {
    let _ = ACTIVE.try_with(|active| {
        if active.get() {
            let _ = COUNTS.try_with(|counts| {
                let mut value = counts.get();
                value[index] += 1;
                counts.set(value);
            });
        }
    });
}
pub(crate) fn codec() {
    mark(2);
}
pub(crate) fn owner() {
    mark(3);
}
pub(crate) fn payload_drop() {
    mark(4);
}
pub(crate) fn begin() {
    COUNTS.with(|c| c.set([0; 5]));
    ACTIVE.with(|a| a.set(true));
}
pub(crate) fn end() {
    ACTIVE.with(|a| a.set(false));
}
pub(crate) fn counts() -> [usize; 5] {
    COUNTS.with(Cell::get)
}

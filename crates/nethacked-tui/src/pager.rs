//! Paging for the inventory modal (20 items per page, letters a-t).

pub const PAGE_SIZE: usize = 20;

#[derive(Debug, Default, Clone, Copy)]
pub struct Pager {
    pub page: usize,
}

impl Pager {
    /// Number of pages needed for `len` items (at least 1).
    pub fn page_count(len: usize) -> usize {
        len.div_ceil(PAGE_SIZE).max(1)
    }

    /// The slice of `items` visible on the current page.
    pub fn page_items<'a, T>(&self, items: &'a [T]) -> &'a [T] {
        let start = (self.page * PAGE_SIZE).min(items.len());
        let end = (start + PAGE_SIZE).min(items.len());
        &items[start..end]
    }

    pub fn next(&mut self, len: usize) {
        if self.page + 1 < Self::page_count(len) {
            self.page += 1;
        }
    }

    pub fn prev(&mut self) {
        self.page = self.page.saturating_sub(1);
    }

    /// Map a letter `a`..=`t` to an absolute item index on the current page.
    pub fn select(&self, letter: char, len: usize) -> Option<usize> {
        if !('a'..='t').contains(&letter) {
            return None;
        }
        let idx = self.page * PAGE_SIZE + (letter as u8 - b'a') as usize;
        (idx < len).then_some(idx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pager_pages_and_selects_visible_only() {
        let items: Vec<u32> = (0..45).collect();
        let mut p = Pager { page: 0 };
        assert_eq!(p.page_items(&items).len(), 20);
        assert_eq!(p.select('t', items.len()), Some(19));
        p.next(items.len());
        p.next(items.len());
        assert_eq!(p.page_items(&items), &items[40..45]);
        assert_eq!(p.select('a', items.len()), Some(40));
        assert_eq!(p.select('f', items.len()), None);
        p.next(items.len());
        assert_eq!(p.page, 2, "no page past the end");
        p.prev();
        p.prev();
        p.prev();
        assert_eq!(p.page, 0);
    }
}

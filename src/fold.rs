//! Manual code folding: inclusive line ranges that collapse to a single display
//! row. This module is pure data + geometry — it never touches the buffer or the
//! screen, so every rule below is exercised directly by the unit tests. The
//! editor layers cursor movement and the renderer layers drawing on top.
//!
//! Folds are kept nested-or-disjoint: a [`Folds::create`] that would partially
//! overlap an existing fold (crossing one of its boundaries) is rejected, which
//! keeps the "outermost closed fold wins" rule below unambiguous.

/// A fold over the inclusive line range `[start, end]` (0-based). An open fold
/// shows its lines normally; a closed fold collapses them to a one-line header
/// drawn on `start`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fold {
    pub start: usize,
    pub end: usize,
    pub open: bool,
}

/// All folds for a buffer, kept sorted outermost-first (smaller `start`, then
/// larger `end`).
#[derive(Default, Clone)]
pub struct Folds {
    items: Vec<Fold>,
}

impl Folds {
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Remove every fold (`zE`).
    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// All folds, outermost-first (for tests and inspection).
    pub fn all(&self) -> &[Fold] {
        &self.items
    }

    fn sort(&mut self) {
        self.items
            .sort_by(|a, b| a.start.cmp(&b.start).then(b.end.cmp(&a.end)));
    }

    /// Create a closed fold over `[start, end]`. Requires at least two lines
    /// (`end > start`) and rejects a range that partially overlaps an existing
    /// fold or exactly duplicates one. Returns whether a fold was added.
    pub fn create(&mut self, start: usize, end: usize) -> bool {
        if end <= start {
            return false;
        }
        for f in &self.items {
            if start == f.start && end == f.end {
                return false; // duplicate
            }
            let disjoint = end < f.start || start > f.end;
            let new_inside_old = start >= f.start && end <= f.end;
            let old_inside_new = f.start >= start && f.end <= end;
            if !(disjoint || new_inside_old || old_inside_new) {
                return false; // partial overlap
            }
        }
        self.items.push(Fold { start, end, open: false });
        self.sort();
        true
    }

    /// Whether `line` is collapsed out of view by some closed fold (i.e. it is
    /// below a closed fold's header). The header line itself is never hidden.
    pub fn is_hidden(&self, line: usize) -> bool {
        self.items
            .iter()
            .any(|f| !f.open && f.start < line && line <= f.end)
    }

    /// If a displayed closed fold begins at `line`, the collapse range it covers
    /// as `(start, end)`; `None` when `line` is an ordinary visible line.
    pub fn header(&self, line: usize) -> Option<(usize, usize)> {
        if self.is_hidden(line) {
            return None;
        }
        let end = self
            .items
            .iter()
            .filter(|f| !f.open && f.start == line)
            .map(|f| f.end)
            .max()?;
        Some((line, end))
    }

    /// The display row that represents `line`: the start of the outermost closed
    /// fold covering it, or `line` itself when it is already visible.
    pub fn display_line(&self, line: usize) -> usize {
        self.items
            .iter()
            .filter(|f| !f.open && f.start < line && line <= f.end)
            .map(|f| f.start)
            .min()
            .unwrap_or(line)
    }

    /// The next displayed line after the row shown at `line` (assumed visible).
    /// Steps over a closed fold's hidden body. May return a line past the last
    /// buffer line; callers clamp.
    pub fn next_visible(&self, line: usize) -> usize {
        match self.header(line) {
            Some((_, end)) => end + 1,
            None => line + 1,
        }
    }

    /// The previous displayed line before `line` (assumed visible).
    pub fn prev_visible(&self, line: usize) -> usize {
        if line == 0 {
            return 0;
        }
        self.display_line(line - 1)
    }

    /// Close the innermost open fold containing `line` (`zc`); returns its start.
    pub fn close_at(&mut self, line: usize) -> Option<usize> {
        let idx = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, f)| f.open && f.start <= line && line <= f.end)
            .max_by_key(|(_, f)| f.start)
            .map(|(i, _)| i)?;
        self.items[idx].open = false;
        Some(self.items[idx].start)
    }

    /// Open the outermost closed fold containing `line` (`zo`); returns its start.
    pub fn open_at(&mut self, line: usize) -> Option<usize> {
        let idx = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, f)| !f.open && f.start <= line && line <= f.end)
            .min_by_key(|(_, f)| f.start)
            .map(|(i, _)| i)?;
        self.items[idx].open = true;
        Some(self.items[idx].start)
    }

    /// Toggle the fold at `line` (`za`): open the outermost closed fold if one
    /// affects this row, otherwise close the innermost open fold. Returns the
    /// affected fold's start.
    pub fn toggle_at(&mut self, line: usize) -> Option<usize> {
        if self.is_hidden(line) || self.header(line).is_some() {
            self.open_at(line)
        } else {
            self.close_at(line)
        }
    }

    /// Delete the innermost fold containing `line` (`zd`). Returns whether one
    /// was removed.
    pub fn delete_at(&mut self, line: usize) -> bool {
        let idx = self
            .items
            .iter()
            .enumerate()
            .filter(|(_, f)| f.start <= line && line <= f.end)
            .max_by_key(|(_, f)| f.start)
            .map(|(i, _)| i);
        match idx {
            Some(i) => {
                self.items.remove(i);
                true
            }
            None => false,
        }
    }

    /// Open every fold (`zR`).
    pub fn open_all(&mut self) {
        for f in &mut self.items {
            f.open = true;
        }
    }

    /// Close every fold (`zM`).
    pub fn close_all(&mut self) {
        for f in &mut self.items {
            f.open = false;
        }
    }

    /// Nesting depth at `line` (how many folds contain it), at least 1. Drives
    /// the width of the `+--` marker on a fold header.
    pub fn level(&self, line: usize) -> usize {
        self.items
            .iter()
            .filter(|f| f.start <= line && line <= f.end)
            .count()
            .max(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_requires_two_lines_and_rejects_duplicates() {
        let mut f = Folds::default();
        assert!(!f.create(3, 3)); // single line
        assert!(!f.create(5, 2)); // inverted
        assert!(f.create(2, 5));
        assert!(!f.create(2, 5)); // duplicate
        assert_eq!(f.all().len(), 1);
    }

    #[test]
    fn create_allows_nested_and_disjoint_rejects_partial_overlap() {
        let mut f = Folds::default();
        assert!(f.create(0, 10));
        assert!(f.create(2, 4)); // nested
        assert!(f.create(12, 15)); // disjoint
        assert!(!f.create(8, 12)); // crosses the [0,10] boundary
        assert_eq!(f.all().len(), 3);
    }

    #[test]
    fn closed_fold_hides_body_but_not_header() {
        let mut f = Folds::default();
        f.create(2, 5);
        assert!(!f.is_hidden(2)); // header
        assert!(f.is_hidden(3));
        assert!(f.is_hidden(5));
        assert!(!f.is_hidden(6));
        assert_eq!(f.header(2), Some((2, 5)));
        assert_eq!(f.header(3), None);
    }

    #[test]
    fn open_fold_hides_nothing() {
        let mut f = Folds::default();
        f.create(2, 5);
        f.open_at(3);
        assert!(!f.is_hidden(3));
        assert_eq!(f.header(2), None);
    }

    #[test]
    fn nested_outer_closed_hides_everything_after_its_start() {
        let mut f = Folds::default();
        f.create(0, 10);
        f.create(2, 4);
        // Outer closed: only line 0 shows, collapsing to 10.
        assert_eq!(f.header(0), Some((0, 10)));
        assert!(f.is_hidden(2));
        assert_eq!(f.display_line(4), 0);
        assert_eq!(f.next_visible(0), 11);
    }

    #[test]
    fn nested_inner_closed_outer_open() {
        let mut f = Folds::default();
        f.create(0, 10);
        f.create(2, 4);
        f.open_at(0); // open outermost at line 0
        // Line 0,1 visible; line 2 is the inner header collapsing to 4; 5.. visible.
        assert_eq!(f.header(0), None);
        assert_eq!(f.header(2), Some((2, 4)));
        assert!(f.is_hidden(3));
        assert!(!f.is_hidden(5));
        assert_eq!(f.next_visible(1), 2);
        assert_eq!(f.next_visible(2), 5);
    }

    #[test]
    fn navigation_steps_over_closed_folds() {
        let mut f = Folds::default();
        f.create(2, 5);
        assert_eq!(f.next_visible(1), 2);
        assert_eq!(f.next_visible(2), 6); // skip the body
        assert_eq!(f.prev_visible(6), 2); // land on the header, not inside
        assert_eq!(f.prev_visible(2), 1);
    }

    #[test]
    fn toggle_opens_then_closes() {
        let mut f = Folds::default();
        f.create(2, 5);
        assert_eq!(f.toggle_at(2), Some(2)); // open it
        assert!(!f.is_hidden(3));
        assert_eq!(f.toggle_at(2), Some(2)); // close it again
        assert!(f.is_hidden(3));
    }

    #[test]
    fn open_close_all_and_delete() {
        let mut f = Folds::default();
        f.create(0, 3);
        f.create(5, 8);
        f.open_all();
        assert!(!f.is_hidden(1) && !f.is_hidden(6));
        f.close_all();
        assert!(f.is_hidden(1) && f.is_hidden(6));
        assert!(f.delete_at(6));
        assert!(!f.is_hidden(6));
        assert_eq!(f.all().len(), 1);
        f.clear();
        assert!(f.is_empty());
    }

    #[test]
    fn delete_removes_innermost() {
        let mut f = Folds::default();
        f.create(0, 10);
        f.create(2, 4);
        assert!(f.delete_at(3)); // removes inner [2,4]
        assert_eq!(f.all().len(), 1);
        assert_eq!(f.all()[0], Fold { start: 0, end: 10, open: false });
    }

    #[test]
    fn level_counts_nesting() {
        let mut f = Folds::default();
        f.create(0, 10);
        f.create(2, 4);
        assert_eq!(f.level(3), 2);
        assert_eq!(f.level(0), 1);
        assert_eq!(f.level(20), 1); // never zero
    }
}

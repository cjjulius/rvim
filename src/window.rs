//! Window layout manager: the geometry behind split windows (`:split` /
//! `:vsplit`). This module is pure — it knows nothing about buffers, editors or
//! drawing. It models the screen's text area as a tree of splits and turns that
//! tree into a rectangle for each window, plus the directional-focus and
//! open/close operations the editor drives on top. Every rule here is exercised
//! directly by the unit tests at the bottom; the renderer and the app consume
//! the rectangles it produces.
//!
//! The tree is n-ary: splitting in the same direction as a window's parent adds
//! a sibling rather than nesting deeper, which matches vim (repeated `:split`s
//! stack as equal siblings). Siblings divide their parent's space equally.

/// A stable identifier for a window. The app maps each id to the editor shown in
/// that window; this module only ever moves ids around.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct WinId(pub usize);

/// How a split divides space, named for the vim command that creates it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SplitKind {
    /// `:split` — windows stacked top-to-bottom (a horizontal divider). Each
    /// window carries its own status line, so no separator row is reserved.
    Horizontal,
    /// `:vsplit` — windows side by side (a vertical divider). A one-column
    /// separator sits between adjacent windows.
    Vertical,
}

/// A direction for `Ctrl-w h/j/k/l` focus movement.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Move {
    Left,
    Right,
    Up,
    Down,
}

/// A rectangle of terminal cells: top-left at `(x, y)`, `w` wide and `h` tall.
/// For a window this is its whole region, including the row its status line is
/// drawn on.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}

impl Rect {
    pub fn new(x: u16, y: u16, w: u16, h: u16) -> Self {
        Self { x, y, w, h }
    }

    /// The center point, used for directional-focus distance tie-breaks.
    fn center(&self) -> (i32, i32) {
        (
            self.x as i32 + self.w as i32 / 2,
            self.y as i32 + self.h as i32 / 2,
        )
    }
}

/// A node in the split tree: either a single window or an ordered group of
/// children laid out along one axis.
#[derive(Clone, Debug)]
enum Node {
    Leaf(WinId),
    /// A group of children laid out along one axis. `weights` (one per child,
    /// all >= 1) set the proportional sizes; equal weights divide evenly. Any
    /// structural change resets weights to equal — only `resize` makes them differ.
    Split {
        kind: SplitKind,
        children: Vec<Node>,
        weights: Vec<u16>,
    },
}

/// The window layout: a split tree over the text area.
#[derive(Clone, Debug)]
pub struct Layout {
    root: Node,
}

impl Layout {
    /// A layout with a single window filling the whole area.
    pub fn new(first: WinId) -> Self {
        Self {
            root: Node::Leaf(first),
        }
    }

    /// Number of windows in the layout.
    pub fn count(&self) -> usize {
        Self::ids_of(&self.root).len()
    }

    /// Whether a window is present.
    pub fn contains(&self, id: WinId) -> bool {
        Self::ids_of(&self.root).contains(&id)
    }

    /// Every window id, in layout order (left-to-right, top-to-bottom).
    pub fn ids(&self) -> Vec<WinId> {
        Self::ids_of(&self.root)
    }

    fn ids_of(node: &Node) -> Vec<WinId> {
        match node {
            Node::Leaf(id) => vec![*id],
            Node::Split { children, .. } => {
                children.iter().flat_map(Self::ids_of).collect()
            }
        }
    }

    /// Split the window `focused` with a new window `new_id`, which vim places
    /// above (`Horizontal`) or to the left (`Vertical`) of the old one and gives
    /// focus. If `focused` is missing, nothing happens. Returns whether it split.
    pub fn split(&mut self, focused: WinId, kind: SplitKind, new_id: WinId) -> bool {
        self.split_dir(focused, kind, new_id, true)
    }

    /// Like [`split`](Layout::split) but `before` chooses the new window's side:
    /// `true` places it above/left (vim default), `false` below/right
    /// (`splitbelow` / `splitright`).
    pub fn split_dir(&mut self, focused: WinId, kind: SplitKind, new_id: WinId, before: bool) -> bool {
        if !self.contains(focused) || self.contains(new_id) {
            return false;
        }
        Self::split_node(&mut self.root, focused, kind, new_id, before);
        true
    }

    /// Returns whether the split happened within `node`.
    fn split_node(
        node: &mut Node,
        focused: WinId,
        kind: SplitKind,
        new_id: WinId,
        before: bool,
    ) -> bool {
        match node {
            Node::Leaf(id) if *id == focused => {
                // Replace this leaf with a split; order puts the new window
                // above/left (`before`) or below/right.
                let old = *id;
                let children = if before {
                    vec![Node::Leaf(new_id), Node::Leaf(old)]
                } else {
                    vec![Node::Leaf(old), Node::Leaf(new_id)]
                };
                *node = Node::Split { kind, children, weights: vec![1, 1] };
                true
            }
            Node::Leaf(_) => false,
            Node::Split {
                kind: parent_kind,
                children,
                weights,
            } => {
                // If a direct child is the focused leaf and this split runs along
                // the same axis, insert the new window as a sibling next to it
                // rather than nesting a fresh split.
                if *parent_kind == kind {
                    if let Some(pos) = children
                        .iter()
                        .position(|c| matches!(c, Node::Leaf(id) if *id == focused))
                    {
                        let at = if before { pos } else { pos + 1 };
                        children.insert(at, Node::Leaf(new_id));
                        *weights = vec![1; children.len()];
                        return true;
                    }
                }
                children
                    .iter_mut()
                    .any(|c| Self::split_node(c, focused, kind, new_id, before))
            }
        }
    }

    /// Close the window `id`. Collapses a split that drops to a single child, and
    /// refuses to close the last remaining window. Returns whether it closed.
    pub fn close(&mut self, id: WinId) -> bool {
        if !self.contains(id) || self.count() == 1 {
            return false;
        }
        Self::remove_node(&mut self.root, id);
        Self::collapse(&mut self.root);
        true
    }

    fn remove_node(node: &mut Node, id: WinId) {
        if let Node::Split { children, .. } = node {
            children.retain(|c| !matches!(c, Node::Leaf(x) if *x == id));
            for c in children.iter_mut() {
                Self::remove_node(c, id);
            }
        }
    }

    /// Flatten any split that now has a single child, and merge a child split
    /// whose kind matches its parent back into the parent (keeps the tree
    /// canonical so geometry and navigation stay predictable).
    fn collapse(node: &mut Node) {
        if let Node::Split { kind, children, weights } = node {
            for c in children.iter_mut() {
                Self::collapse(c);
            }
            // Merge same-kind child splits into this one.
            let parent_kind = *kind;
            let mut merged: Vec<Node> = Vec::new();
            for child in std::mem::take(children) {
                match child {
                    Node::Split {
                        kind: ck,
                        children: gc,
                        ..
                    } if ck == parent_kind => merged.extend(gc),
                    other => merged.push(other),
                }
            }
            *children = merged;
            // Structural change: reset to even sizing.
            *weights = vec![1; children.len()];
            // A split with one child is just that child.
            if children.len() == 1 {
                let only = children.pop().unwrap();
                *node = only;
            }
        }
    }

    /// `Ctrl-w o` / `:only` — drop every window but `keep`.
    pub fn only(&mut self, keep: WinId) {
        if self.contains(keep) {
            self.root = Node::Leaf(keep);
        }
    }

    /// Each window's rectangle within `area`, in layout order. Side-by-side
    /// windows leave a one-column separator between them; stacked windows abut
    /// (their status lines divide them).
    pub fn rects(&self, area: Rect) -> Vec<(WinId, Rect)> {
        let mut out = Vec::new();
        Self::rects_of(&self.root, area, &mut out);
        out
    }

    fn rects_of(node: &Node, area: Rect, out: &mut Vec<(WinId, Rect)>) {
        match node {
            Node::Leaf(id) => out.push((*id, area)),
            Node::Split { kind, children, weights } => {
                let n = children.len() as u16;
                if n == 0 {
                    return;
                }
                match kind {
                    SplitKind::Vertical => {
                        let seps = n - 1; // one separator column between windows
                        let usable = area.w.saturating_sub(seps);
                        let sizes = divide_weighted(usable, weights);
                        let mut x = area.x;
                        for (child, w) in children.iter().zip(sizes) {
                            Self::rects_of(child, Rect::new(x, area.y, w, area.h), out);
                            x = x.saturating_add(w).saturating_add(1); // + separator
                        }
                    }
                    SplitKind::Horizontal => {
                        let sizes = divide_weighted(area.h, weights);
                        let mut y = area.y;
                        for (child, h) in children.iter().zip(sizes) {
                            Self::rects_of(child, Rect::new(area.x, y, area.w, h), out);
                            y = y.saturating_add(h);
                        }
                    }
                }
            }
        }
    }

    /// The window to focus when moving `dir` from `from`, computed from the
    /// rectangles over `area`. Picks the nearest window on that side whose
    /// perpendicular span overlaps `from`; falls back to the nearest by center.
    pub fn neighbor(&self, from: WinId, dir: Move, area: Rect) -> Option<WinId> {
        let rects = self.rects(area);
        let me = rects.iter().find(|(id, _)| *id == from)?.1;
        let mut best: Option<(WinId, i32)> = None;
        for (id, r) in &rects {
            if *id == from {
                continue;
            }
            let on_side = match dir {
                Move::Left => r.x + r.w <= me.x && overlaps_v(r, &me),
                Move::Right => r.x >= me.x + me.w && overlaps_v(r, &me),
                Move::Up => r.y + r.h <= me.y && overlaps_h(r, &me),
                Move::Down => r.y >= me.y + me.h && overlaps_h(r, &me),
            };
            if !on_side {
                continue;
            }
            let (mx, my) = me.center();
            let (cx, cy) = r.center();
            let dist = match dir {
                Move::Left | Move::Right => (cx - mx).abs() * 1000 + (cy - my).abs(),
                Move::Up | Move::Down => (cy - my).abs() * 1000 + (cx - mx).abs(),
            };
            if best.is_none_or(|(_, d)| dist < d) {
                best = Some((*id, dist));
            }
        }
        best.map(|(id, _)| id)
    }

    /// `Ctrl-w w` — the next window in layout order (wraps around).
    pub fn next(&self, from: WinId) -> Option<WinId> {
        let ids = self.ids();
        let pos = ids.iter().position(|id| *id == from)?;
        Some(ids[(pos + 1) % ids.len()])
    }

    /// `Ctrl-w W` — the previous window in layout order (wraps around).
    pub fn prev(&self, from: WinId) -> Option<WinId> {
        let ids = self.ids();
        let pos = ids.iter().position(|id| *id == from)?;
        Some(ids[(pos + ids.len() - 1) % ids.len()])
    }

    /// Resize window `from` by `delta` steps along `kind`'s axis: adjusts its
    /// weight within the nearest enclosing split of that kind (height for
    /// `Horizontal`, width for `Vertical`). A positive delta grows it. No-op when
    /// there is no such enclosing split. Returns whether anything changed.
    pub fn resize(&mut self, from: WinId, kind: SplitKind, delta: i16) -> bool {
        matches!(
            Self::resize_node(&mut self.root, from, kind, delta),
            ResizeResult::Done
        )
    }

    fn resize_node(node: &mut Node, from: WinId, kind: SplitKind, delta: i16) -> ResizeResult {
        match node {
            Node::Leaf(id) if *id == from => ResizeResult::InSubtree,
            Node::Leaf(_) => ResizeResult::NotFound,
            Node::Split { kind: k, children, weights } => {
                for i in 0..children.len() {
                    match Self::resize_node(&mut children[i], from, kind, delta) {
                        ResizeResult::Done => return ResizeResult::Done,
                        ResizeResult::InSubtree => {
                            // The focused window is in child `i`. The nearest
                            // enclosing split of the requested kind adjusts it.
                            if *k == kind {
                                let w = weights[i] as i16 + delta;
                                weights[i] = w.max(1) as u16;
                                return ResizeResult::Done;
                            }
                            return ResizeResult::InSubtree;
                        }
                        ResizeResult::NotFound => {}
                    }
                }
                ResizeResult::NotFound
            }
        }
    }

    /// `Ctrl-w =` — reset every split to even sizing.
    pub fn equalize(&mut self) {
        Self::equalize_node(&mut self.root);
    }

    fn equalize_node(node: &mut Node) {
        if let Node::Split { children, weights, .. } = node {
            *weights = vec![1; children.len()];
            for c in children.iter_mut() {
                Self::equalize_node(c);
            }
        }
    }
}

/// Result of walking the tree to resize a window.
#[derive(PartialEq, Eq)]
enum ResizeResult {
    /// The weight was adjusted.
    Done,
    /// The target is in this subtree but no matching-kind split consumed it yet.
    InSubtree,
    /// The target is not in this subtree.
    NotFound,
}

/// Whether two rects overlap on the vertical axis (share some rows).
fn overlaps_v(a: &Rect, b: &Rect) -> bool {
    a.y < b.y + b.h && b.y < a.y + a.h
}

/// Whether two rects overlap on the horizontal axis (share some columns).
fn overlaps_h(a: &Rect, b: &Rect) -> bool {
    a.x < b.x + b.w && b.x < a.x + a.w
}

/// Divide `total` cells among slices in proportion to `weights` (each clamped to
/// at least 1), using largest-remainder apportionment with ties going to the
/// earlier slice. Equal weights reproduce an even split with the remainder handed
/// to the first slices.
fn divide_weighted(total: u16, weights: &[u16]) -> Vec<u16> {
    let n = weights.len();
    if n == 0 {
        return Vec::new();
    }
    let w: Vec<u32> = weights.iter().map(|&x| x.max(1) as u32).collect();
    let sum: u32 = w.iter().sum();
    let mut sizes = vec![0u16; n];
    let mut remainder = vec![0u32; n];
    let mut used = 0u16;
    for i in 0..n {
        let exact = total as u32 * w[i];
        sizes[i] = (exact / sum) as u16;
        remainder[i] = exact % sum;
        used += sizes[i];
    }
    // Hand out the leftover cells to the largest remainders (ties: lower index).
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| remainder[b].cmp(&remainder[a]).then(a.cmp(&b)));
    let mut leftover = total.saturating_sub(used);
    let mut k = 0;
    while leftover > 0 {
        sizes[order[k % n]] += 1;
        leftover -= 1;
        k += 1;
    }
    sizes
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: Rect = Rect {
        x: 0,
        y: 0,
        w: 80,
        h: 24,
    };

    fn ids(v: &[(WinId, Rect)]) -> Vec<usize> {
        v.iter().map(|(WinId(n), _)| *n).collect()
    }

    #[test]
    fn single_window_fills_area() {
        let l = Layout::new(WinId(0));
        let r = l.rects(AREA);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0], (WinId(0), AREA));
        assert_eq!(l.count(), 1);
    }

    #[test]
    fn divide_weighted_even_puts_remainder_up_front() {
        assert_eq!(divide_weighted(24, &[1, 1]), vec![12, 12]);
        assert_eq!(divide_weighted(25, &[1, 1]), vec![13, 12]);
        assert_eq!(divide_weighted(10, &[1, 1, 1]), vec![4, 3, 3]);
        assert_eq!(divide_weighted(0, &[1, 1, 1]), vec![0, 0, 0]);
    }

    #[test]
    fn resize_adjusts_focused_window_along_its_axis() {
        let mut l = Layout::new(WinId(0));
        l.split(WinId(0), SplitKind::Horizontal, WinId(1)); // [1, 0] stacked, 12/12
        assert!(l.resize(WinId(1), SplitKind::Horizontal, 1)); // weights [2, 1]
        let r = l.rects(AREA);
        let h = |n: usize| r.iter().find(|(WinId(x), _)| *x == n).unwrap().1.h;
        assert_eq!(h(1), 16);
        assert_eq!(h(0), 8);
    }

    #[test]
    fn resize_on_wrong_axis_is_noop() {
        let mut l = Layout::new(WinId(0));
        l.split(WinId(0), SplitKind::Horizontal, WinId(1)); // only a horizontal split
        assert!(!l.resize(WinId(1), SplitKind::Vertical, 1));
    }

    #[test]
    fn equalize_restores_even_sizing() {
        let mut l = Layout::new(WinId(0));
        l.split(WinId(0), SplitKind::Horizontal, WinId(1));
        l.resize(WinId(1), SplitKind::Horizontal, 3);
        l.equalize();
        let r = l.rects(AREA);
        assert_eq!(r.iter().map(|(_, rc)| rc.h).collect::<Vec<_>>(), vec![12, 12]);
    }

    #[test]
    fn a_structural_change_resets_weights() {
        let mut l = Layout::new(WinId(0));
        l.split(WinId(0), SplitKind::Horizontal, WinId(1));
        l.resize(WinId(1), SplitKind::Horizontal, 2); // skew sizes
        l.split(WinId(1), SplitKind::Horizontal, WinId(2)); // sibling insert resets
        let r = l.rects(AREA);
        assert_eq!(r.iter().map(|(_, rc)| rc.h).collect::<Vec<_>>(), vec![8, 8, 8]);
    }

    #[test]
    fn divide_weighted_respects_weights() {
        assert_eq!(divide_weighted(24, &[2, 1]), vec![16, 8]);
        assert_eq!(divide_weighted(30, &[1, 2, 3]), vec![5, 10, 15]);
        // A zero weight is clamped to 1 so no window vanishes entirely.
        assert_eq!(divide_weighted(10, &[0, 1]).iter().sum::<u16>(), 10);
    }

    #[test]
    fn horizontal_split_stacks_top_and_bottom() {
        let mut l = Layout::new(WinId(0));
        assert!(l.split(WinId(0), SplitKind::Horizontal, WinId(1)));
        let r = l.rects(AREA);
        assert_eq!(l.count(), 2);
        // New window (1) is on top, old window (0) below.
        assert_eq!(ids(&r), vec![1, 0]);
        let top = r[0].1;
        let bot = r[1].1;
        assert_eq!(top, Rect::new(0, 0, 80, 12));
        assert_eq!(bot, Rect::new(0, 12, 80, 12));
        // Full width each, heights sum to the area, no gap.
        assert_eq!(top.h + bot.h, AREA.h);
        assert_eq!(bot.y, top.y + top.h);
    }

    #[test]
    fn vertical_split_places_side_by_side_with_separator() {
        let mut l = Layout::new(WinId(0));
        assert!(l.split(WinId(0), SplitKind::Vertical, WinId(1)));
        let r = l.rects(AREA);
        // New window (1) is on the left, old (0) on the right.
        assert_eq!(ids(&r), vec![1, 0]);
        let left = r[0].1;
        let right = r[1].1;
        // 80 columns minus 1 separator = 79, split 40/39.
        assert_eq!(left, Rect::new(0, 0, 40, 24));
        assert_eq!(right, Rect::new(41, 0, 39, 24));
        // One column gap between them for the separator.
        assert_eq!(right.x, left.x + left.w + 1);
    }

    #[test]
    fn split_dir_places_new_window_after_when_not_before() {
        let mut l = Layout::new(WinId(0));
        // before=false -> new window below/right of the old one.
        l.split_dir(WinId(0), SplitKind::Horizontal, WinId(1), false);
        assert_eq!(l.ids(), vec![WinId(0), WinId(1)]); // old first, new second
        let r = l.rects(AREA);
        assert_eq!(ids(&r), vec![0, 1]);
        // A same-axis sibling also inserts after the focused window.
        l.split_dir(WinId(1), SplitKind::Horizontal, WinId(2), false);
        assert_eq!(l.ids(), vec![WinId(0), WinId(1), WinId(2)]);
    }

    #[test]
    fn repeated_same_direction_splits_are_equal_siblings() {
        let mut l = Layout::new(WinId(0));
        l.split(WinId(0), SplitKind::Horizontal, WinId(1));
        l.split(WinId(1), SplitKind::Horizontal, WinId(2));
        assert_eq!(l.count(), 3);
        let r = l.rects(AREA);
        // Three stacked windows, heights 8/8/8.
        assert_eq!(r.iter().map(|(_, rc)| rc.h).collect::<Vec<_>>(), vec![8, 8, 8]);
        // They tile the height with no gap.
        assert_eq!(r[0].1.y, 0);
        assert_eq!(r[1].1.y, 8);
        assert_eq!(r[2].1.y, 16);
    }

    #[test]
    fn nested_mixed_splits() {
        // Split vertically, then split the left window horizontally.
        let mut l = Layout::new(WinId(0));
        l.split(WinId(0), SplitKind::Vertical, WinId(1)); // [1 | 0]
        l.split(WinId(1), SplitKind::Horizontal, WinId(2)); // left column: 2 over 1
        assert_eq!(l.count(), 3);
        let r = l.rects(AREA);
        let get = |n: usize| r.iter().find(|(WinId(x), _)| *x == n).unwrap().1;
        // Right window spans full height.
        assert_eq!(get(0).h, 24);
        // Left column is split into two 12-high windows.
        assert_eq!(get(2), Rect::new(0, 0, 40, 12));
        assert_eq!(get(1), Rect::new(0, 12, 40, 12));
    }

    #[test]
    fn neighbor_navigation_side_by_side() {
        let mut l = Layout::new(WinId(0));
        l.split(WinId(0), SplitKind::Vertical, WinId(1)); // [1 | 0]
        assert_eq!(l.neighbor(WinId(1), Move::Right, AREA), Some(WinId(0)));
        assert_eq!(l.neighbor(WinId(0), Move::Left, AREA), Some(WinId(1)));
        // No window above/below a full-height pane.
        assert_eq!(l.neighbor(WinId(1), Move::Up, AREA), None);
        assert_eq!(l.neighbor(WinId(0), Move::Left, AREA), Some(WinId(1)));
        assert_eq!(l.neighbor(WinId(1), Move::Left, AREA), None);
    }

    #[test]
    fn neighbor_navigation_stacked() {
        let mut l = Layout::new(WinId(0));
        l.split(WinId(0), SplitKind::Horizontal, WinId(1)); // 1 over 0
        assert_eq!(l.neighbor(WinId(1), Move::Down, AREA), Some(WinId(0)));
        assert_eq!(l.neighbor(WinId(0), Move::Up, AREA), Some(WinId(1)));
        assert_eq!(l.neighbor(WinId(0), Move::Right, AREA), None);
    }

    #[test]
    fn neighbor_picks_overlapping_pane_in_a_grid() {
        // Build a 2x2 grid: vsplit, then hsplit each column.
        let mut l = Layout::new(WinId(0));
        l.split(WinId(0), SplitKind::Vertical, WinId(1)); // [1 | 0]
        l.split(WinId(1), SplitKind::Horizontal, WinId(2)); // left: 2 over 1
        l.split(WinId(0), SplitKind::Horizontal, WinId(3)); // right: 3 over 0
        // Layout: top row [2 | 3], bottom row [1 | 0].
        assert_eq!(l.neighbor(WinId(2), Move::Right, AREA), Some(WinId(3)));
        assert_eq!(l.neighbor(WinId(2), Move::Down, AREA), Some(WinId(1)));
        assert_eq!(l.neighbor(WinId(3), Move::Down, AREA), Some(WinId(0)));
        assert_eq!(l.neighbor(WinId(0), Move::Left, AREA), Some(WinId(1)));
        assert_eq!(l.neighbor(WinId(0), Move::Up, AREA), Some(WinId(3)));
    }

    #[test]
    fn close_collapses_the_parent_split() {
        let mut l = Layout::new(WinId(0));
        l.split(WinId(0), SplitKind::Vertical, WinId(1));
        assert!(l.close(WinId(1)));
        assert_eq!(l.count(), 1);
        // The surviving window fills the whole area again.
        assert_eq!(l.rects(AREA), vec![(WinId(0), AREA)]);
    }

    #[test]
    fn close_last_window_is_refused() {
        let mut l = Layout::new(WinId(0));
        assert!(!l.close(WinId(0)));
        assert_eq!(l.count(), 1);
    }

    #[test]
    fn close_one_of_three_keeps_the_rest() {
        let mut l = Layout::new(WinId(0));
        l.split(WinId(0), SplitKind::Horizontal, WinId(1));
        l.split(WinId(1), SplitKind::Horizontal, WinId(2)); // [2,1,0] stacked
        assert!(l.close(WinId(1)));
        assert_eq!(l.ids(), vec![WinId(2), WinId(0)]);
        let r = l.rects(AREA);
        // Two windows now split the height evenly.
        assert_eq!(r.iter().map(|(_, rc)| rc.h).collect::<Vec<_>>(), vec![12, 12]);
    }

    #[test]
    fn only_drops_every_other_window() {
        let mut l = Layout::new(WinId(0));
        l.split(WinId(0), SplitKind::Vertical, WinId(1));
        l.split(WinId(1), SplitKind::Horizontal, WinId(2));
        l.only(WinId(0));
        assert_eq!(l.count(), 1);
        assert_eq!(l.rects(AREA), vec![(WinId(0), AREA)]);
    }

    #[test]
    fn next_and_prev_wrap_in_layout_order() {
        let mut l = Layout::new(WinId(0));
        l.split(WinId(0), SplitKind::Vertical, WinId(1)); // order [1, 0]
        assert_eq!(l.next(WinId(1)), Some(WinId(0)));
        assert_eq!(l.next(WinId(0)), Some(WinId(1))); // wraps
        assert_eq!(l.prev(WinId(1)), Some(WinId(0))); // wraps
        assert_eq!(l.prev(WinId(0)), Some(WinId(1)));
    }

    #[test]
    fn split_rejects_unknown_or_duplicate_ids() {
        let mut l = Layout::new(WinId(0));
        assert!(!l.split(WinId(9), SplitKind::Vertical, WinId(1))); // focused missing
        l.split(WinId(0), SplitKind::Vertical, WinId(1));
        assert!(!l.split(WinId(0), SplitKind::Vertical, WinId(1))); // id 1 already used
        assert_eq!(l.count(), 2);
    }

    #[test]
    fn rects_degrade_gracefully_in_a_tiny_area() {
        let mut l = Layout::new(WinId(0));
        l.split(WinId(0), SplitKind::Vertical, WinId(1));
        // Width 1 can't fit two windows plus a separator; no panic, widths are 0.
        let r = l.rects(Rect::new(0, 0, 1, 5));
        assert_eq!(r.len(), 2);
        assert!(r.iter().all(|(_, rc)| rc.w == 0));
    }
}

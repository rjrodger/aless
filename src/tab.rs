//! A tab: one open document and the view onto it (focus, scroll, fold
//! state, mode, search), with the jless navigation semantics and the
//! reload that keeps the reader's place.

use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime};

use crate::doc::{row_of, Doc, Key, NodeId, Row};
use crate::explorer::Explorer;
use crate::fmt;
use crate::load::{self, Format, LoadError, Loaded};
use crate::search::{Direction, Pattern};

/// The file stamp the poll fallback compares: modification time and size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stamp {
    pub mtime: Option<SystemTime>,
    pub len: u64,
}

impl Stamp {
    pub fn of(path: &Path) -> Option<Stamp> {
        let m = std::fs::metadata(path).ok()?;
        Some(Stamp {
            mtime: m.modified().ok(),
            len: m.len(),
        })
    }
}

/// The viewport the navigation works within.
#[derive(Clone, Copy, Debug)]
pub struct View {
    /// Rows available to the tree pane.
    pub height: usize,
    /// Rows kept between the focus and the pane edge when possible.
    pub scrolloff: usize,
}

impl View {
    pub fn new(height: usize, scrolloff: usize) -> View {
        View {
            height: height.max(1),
            scrolloff,
        }
    }

    fn so(&self) -> usize {
        self.scrolloff.min(self.height.saturating_sub(1) / 2)
    }
}

#[derive(Clone, Debug)]
pub struct Search {
    pub pattern: Pattern,
    pub direction: Direction,
    /// Matching nodes, ascending.
    pub matches: Vec<NodeId>,
    /// Index into `matches` of the match last jumped to.
    pub current: Option<usize>,
    pub wrapped: bool,
}

#[derive(Clone, Debug)]
pub struct Tab {
    pub id: u64,
    pub title: String,
    pub path: Option<PathBuf>,
    pub format: Format,
    /// A format given explicitly (`--kind`, `:open p kind`, `:format`),
    /// which reloads keep.
    pub explicit_format: Option<Format>,
    pub doc: Doc,
    pub source: String,
    rows: Vec<Row>,
    rows_dirty: bool,
    pub focus: usize,
    pub scroll: usize,
    pub line_mode: bool,
    /// Horizontal scroll of the focused row's text.
    pub xoff: usize,
    pub watch: bool,
    pub stamp: Option<Stamp>,
    pub error: Option<LoadError>,
    pub gone: bool,
    pub generation: u64,
    pub reload_due: Option<Instant>,
    pub search: Option<Search>,
    /// Source-view scroll offset (first shown line, 0-based).
    pub source_scroll: usize,
    /// Set when this tab shows a directory tree rather than a document.
    pub explorer: Option<Explorer>,
}

impl Tab {
    /// A tab for a loaded document.
    pub fn new(id: u64, title: String, path: Option<PathBuf>, loaded: Loaded) -> Tab {
        let stamp = path.as_deref().and_then(Stamp::of);
        Tab {
            id,
            title,
            path,
            format: loaded.format,
            explicit_format: None,
            doc: loaded.doc,
            source: loaded.source,
            rows: Vec::new(),
            rows_dirty: true,
            focus: 0,
            scroll: 0,
            line_mode: false,
            xoff: 0,
            watch: false,
            stamp,
            error: None,
            gone: false,
            generation: 0,
            reload_due: None,
            search: None,
            source_scroll: 0,
            explorer: None,
        }
    }

    /// A tab exploring a directory.
    pub fn explore(id: u64, dir: &Path, show_hidden: bool) -> Tab {
        let ex = Explorer::open(dir, show_hidden);
        let loaded = Loaded {
            doc: ex.build(),
            format: Format::Text,
            source: String::new(),
        };
        let mut tab = Tab::new(
            id,
            crate::explorer::title_of(&ex.root),
            Some(ex.root.clone()),
            loaded,
        );
        tab.explorer = Some(ex);
        tab
    }

    /// A tab for a file that failed to load: an empty document, the error,
    /// and whatever source text could be read (so the source view can show
    /// the failing line).
    pub fn failed(id: u64, title: String, path: PathBuf, format: Format, err: LoadError) -> Tab {
        let source = std::fs::read_to_string(&path).unwrap_or_default();
        let gone = !path.exists();
        let mut tab = Tab::new(
            id,
            title,
            Some(path),
            Loaded {
                doc: Doc::from_lines(&[]),
                format,
                source,
            },
        );
        tab.error = Some(err);
        tab.gone = gone;
        tab
    }

    pub fn open(id: u64, path: &Path, format: Option<Format>) -> Tab {
        let title = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        let mut tab = match load::load_path(path, format) {
            Ok(loaded) => Tab::new(id, title, Some(path.to_path_buf()), loaded),
            Err(err) => Tab::failed(
                id,
                title,
                path.to_path_buf(),
                format.unwrap_or_else(|| Format::detect(path)),
                err,
            ),
        };
        tab.explicit_format = format;
        tab
    }

    // ----- rows --------------------------------------------------------------

    /// Rebuild the row projection if a fold or mode change dirtied it.
    fn ensure_rows(&mut self) {
        if self.rows_dirty {
            self.rows = self.doc.rows(self.line_mode);
            self.rows_dirty = false;
            if self.rows.is_empty() {
                self.focus = 0;
            } else if self.focus >= self.rows.len() {
                self.focus = self.rows.len() - 1;
            }
        }
    }

    pub fn rows(&mut self) -> &[Row] {
        self.ensure_rows();
        &self.rows
    }

    pub fn row_count(&mut self) -> usize {
        self.rows().len()
    }

    pub fn focused_row(&mut self) -> Option<Row> {
        let f = self.focus;
        self.rows().get(f).copied()
    }

    pub fn focused_node(&mut self) -> NodeId {
        self.focused_row().map(|r| r.node).unwrap_or(0)
    }

    pub fn row_depth(&self, row: Row) -> usize {
        self.doc.node(row.node).depth as usize
    }

    /// Re-run a fold change keeping the focus on the same node (or its
    /// nearest visible ancestor if the change hid it).
    fn refold(&mut self, view: View, change: impl FnOnce(&mut Doc)) {
        let node = self.focused_node();
        change(&mut self.doc);
        self.rows_dirty = true;
        let target = self.doc.first_visible_ancestor(node);
        self.ensure_rows();
        if let Some(r) = row_of(&self.rows, target) {
            self.focus = r;
        }
        self.follow(view);
    }

    /// Focus the open row of `node` (revealing it if hidden).
    pub fn focus_node(&mut self, node: NodeId, view: View) {
        if !self.doc.is_visible(node) {
            self.doc.reveal(node);
            self.rows_dirty = true;
        }
        let rows = self.rows();
        if let Some(r) = row_of(rows, node) {
            self.focus = r;
        }
        self.follow(view);
    }

    /// Pull the window along so the focus is visible, honouring scrolloff.
    pub fn follow(&mut self, view: View) {
        let n = self.row_count();
        if n == 0 {
            self.scroll = 0;
            self.focus = 0;
            return;
        }
        self.focus = self.focus.min(n - 1);
        let so = view.so();
        let h = view.height;
        // The focus must sit in [scroll + so, scroll + h - 1 - so] where the
        // document allows.
        if self.focus < self.scroll + so {
            self.scroll = self.focus.saturating_sub(so);
        } else if self.focus + so >= self.scroll + h {
            self.scroll = self.focus + so + 1 - h;
        }
        self.scroll = self.scroll.min(n.saturating_sub(h));
    }

    /// Move the focus by a signed number of rows, following.
    fn set_focus(&mut self, focus: isize, view: View) {
        let n = self.row_count() as isize;
        let f = focus.clamp(0, (n - 1).max(0)) as usize;
        if f != self.focus {
            self.xoff = 0;
        }
        self.focus = f;
        self.follow(view);
    }

    // ----- movement ---------------------------------------------------------

    pub fn move_down(&mut self, n: usize, view: View) {
        self.set_focus(self.focus as isize + n as isize, view);
    }

    pub fn move_up(&mut self, n: usize, view: View) {
        self.set_focus(self.focus as isize - n as isize, view);
    }

    /// `h`: collapse an expanded container, else go to the parent. On a
    /// closing row, go to its opening row.
    pub fn move_left(&mut self, view: View) {
        let Some(row) = self.focused_row() else {
            return;
        };
        if row.close {
            self.focus_node(row.node, view);
            return;
        }
        let node = self.doc.node(row.node);
        if node.is_foldable() && node.expanded {
            self.refold(view, |d| d.set_expanded(row.node, false));
        } else if let Some(p) = self.doc.parent(row.node) {
            self.focus_node(p, view);
        }
    }

    /// `l`: expand a collapsed container, step into an expanded one, do
    /// nothing on a scalar. On a closing row, move up one row.
    pub fn move_right(&mut self, view: View) {
        let Some(row) = self.focused_row() else {
            return;
        };
        if row.close {
            self.move_up(1, view);
            return;
        }
        let node = self.doc.node(row.node);
        if !node.is_foldable() {
            return;
        }
        if node.expanded {
            self.move_down(1, view);
        } else {
            self.refold(view, |d| d.set_expanded(row.node, true));
        }
    }

    /// `H`: the parent, without collapsing.
    pub fn focus_parent(&mut self, view: View) {
        let node = self.focused_node();
        if let Some(p) = self.doc.parent(node) {
            self.focus_node(p, view);
        }
    }

    /// `J`/`K`: the n-th next/previous sibling (stopping at the last one).
    pub fn sibling(&mut self, n: usize, forward: bool, view: View) {
        let mut node = self.focused_node();
        for _ in 0..n {
            let next = if forward {
                self.doc.next_sibling(node)
            } else {
                self.doc.prev_sibling(node)
            };
            match next {
                Some(s) => node = s,
                None => break,
            }
        }
        self.focus_node(node, view);
    }

    /// `0`/`^`: the first sibling; at the root, the first row.
    pub fn first_sibling(&mut self, view: View) {
        let node = self.focused_node();
        match self.doc.parent(node) {
            Some(_) => {
                let first = self.doc.first_sibling(node);
                self.focus_node(first, view);
            }
            None => self.set_focus(0, view),
        }
    }

    /// `$`: the last sibling; at the root, the last row (its opening, in
    /// line mode).
    pub fn last_sibling(&mut self, view: View) {
        let node = self.focused_node();
        match self.doc.parent(node) {
            Some(_) => {
                let last = self.doc.last_sibling(node);
                self.focus_node(last, view);
            }
            None => {
                let rows = self.rows();
                let last_open = rows.iter().rposition(|r| !r.close).unwrap_or(0);
                self.set_focus(last_open as isize, view);
            }
        }
    }

    /// `w`/`b`: the next/previous row at a different depth.
    pub fn depth_change(&mut self, n: usize, forward: bool, view: View) {
        for _ in 0..n {
            let Some(row) = self.focused_row() else {
                return;
            };
            let depth = self.row_depth(row);
            self.ensure_rows();
            let focus = self.focus;
            let doc = &self.doc;
            let rows = &self.rows;
            let found = if forward {
                rows[focus + 1..]
                    .iter()
                    .position(|r| doc.node(r.node).depth as usize != depth)
                    .map(|i| focus + 1 + i)
            } else {
                rows[..focus]
                    .iter()
                    .rposition(|r| doc.node(r.node).depth as usize != depth)
            };
            match found {
                Some(f) => self.set_focus(f as isize, view),
                None => break,
            }
        }
    }

    /// `g`/`Home`: the first row.
    pub fn top(&mut self, view: View) {
        self.set_focus(0, view);
    }

    /// `G`/`End`: the last row.
    pub fn bottom(&mut self, view: View) {
        let n = self.row_count();
        self.set_focus(n as isize - 1, view);
    }

    /// `Ng`/`NG`: the N-th visible row, 1-based.
    pub fn goto_row(&mut self, n: usize, view: View) {
        self.set_focus(n.saturating_sub(1) as isize, view);
    }

    // ----- scrolling --------------------------------------------------------

    /// `Ctrl-e`/`Ctrl-y`: move the window, dragging the focus only when it
    /// would leave the scrolloff band.
    pub fn scroll_by(&mut self, delta: isize, view: View) {
        let n = self.row_count();
        if n == 0 {
            return;
        }
        let max_scroll = n.saturating_sub(view.height);
        let target = (self.scroll as isize + delta).clamp(0, max_scroll as isize) as usize;
        self.scroll = target;
        let so = view.so();
        let lo = self.scroll + so;
        let hi = (self.scroll + view.height).saturating_sub(1 + so);
        let lo = lo.min(n - 1);
        let hi = hi.min(n - 1).max(lo);
        if self.focus < lo {
            self.focus = lo;
            self.xoff = 0;
        } else if self.focus > hi {
            self.focus = hi;
            self.xoff = 0;
        }
    }

    /// `Ctrl-f`/`Ctrl-b`, `PageDown`/`PageUp`: a window height at a time.
    pub fn page(&mut self, n: usize, forward: bool, view: View) {
        let delta = (n * view.height) as isize;
        self.scroll_by(if forward { delta } else { -delta }, view);
    }

    /// `Ctrl-d`/`Ctrl-u`: move window and focus together by `distance`
    /// rows; at a boundary only the focus moves. Ignores scrolloff.
    pub fn jump(&mut self, distance: usize, forward: bool, view: View) {
        let n = self.row_count();
        if n == 0 {
            return;
        }
        let max_scroll = n.saturating_sub(view.height);
        let d = distance as isize * if forward { 1 } else { -1 };
        let new_scroll = (self.scroll as isize + d).clamp(0, max_scroll as isize) as usize;
        self.scroll = new_scroll;
        let new_focus = (self.focus as isize + d).clamp(0, n as isize - 1) as usize;
        if new_focus != self.focus {
            self.xoff = 0;
        }
        self.focus = new_focus;
        // Keep the focus inside the window without scrolloff.
        if self.focus < self.scroll {
            self.focus = self.scroll;
        } else if self.focus >= self.scroll + view.height {
            self.focus = self.scroll + view.height - 1;
        }
    }

    /// `zt`/`zz`/`zb`.
    pub fn reposition(&mut self, where_: Reposition, view: View) {
        let n = self.row_count();
        if n == 0 {
            return;
        }
        let so = view.so();
        let h = view.height;
        let want = match where_ {
            Reposition::Top => self.focus as isize - so as isize,
            Reposition::Center => self.focus as isize - (h / 2) as isize,
            Reposition::Bottom => self.focus as isize - (h as isize - 1 - so as isize),
        };
        self.scroll = want.clamp(0, n.saturating_sub(h) as isize) as usize;
    }

    // ----- folding ----------------------------------------------------------

    /// `Space`: toggle the focused container (from a closing row, its
    /// opening row).
    pub fn toggle(&mut self, view: View) {
        let Some(row) = self.focused_row() else {
            return;
        };
        if row.close {
            self.focus_node(row.node, view);
        }
        if self.doc.node(row.node).is_foldable() {
            self.refold(view, |d| d.toggle(row.node));
        }
    }

    /// `c`/`e` (shallow) and `C`/`E` (deep): fold the focused node and its
    /// siblings.
    pub fn fold_siblings(&mut self, expanded: bool, deep: bool, view: View) {
        let Some(row) = self.focused_row() else {
            return;
        };
        if row.close {
            self.focus_node(row.node, view);
        }
        let node = row.node;
        self.refold(view, |d| {
            if deep {
                d.set_siblings_expanded_deep(node, expanded);
            } else {
                d.set_siblings_expanded(node, expanded);
            }
        });
    }

    /// Fold the whole document to a depth (`:depth N`).
    pub fn expand_to_depth(&mut self, depth: u32, view: View) {
        self.refold(view, |d| d.expand_to_depth(depth));
    }

    /// `m`: switch between data and line mode, keeping the focused node.
    pub fn toggle_mode(&mut self, view: View) {
        let Some(row) = self.focused_row() else {
            self.line_mode = !self.line_mode;
            self.rows_dirty = true;
            return;
        };
        let line = self.focus.saturating_sub(self.scroll);
        // Leaving line mode from a closing row: land on the next item.
        let target = if row.close && self.line_mode {
            self.ensure_rows();
            let focus = self.focus;
            let rows = &self.rows;
            rows[focus + 1..]
                .iter()
                .find(|r| !r.close)
                .or_else(|| rows[..focus].iter().rev().find(|r| !r.close))
                .map(|r| r.node)
                .unwrap_or(row.node)
        } else {
            row.node
        };
        self.line_mode = !self.line_mode;
        self.rows_dirty = true;
        let rows = self.rows();
        if let Some(r) = row_of(rows, target) {
            self.focus = r;
        }
        self.scroll = self.focus.saturating_sub(line);
        self.follow(view);
    }

    /// `%`: between a container's opening and closing rows (line mode).
    pub fn matching_pair(&mut self, view: View) {
        if !self.line_mode {
            return;
        }
        let Some(row) = self.focused_row() else {
            return;
        };
        let node = self.doc.node(row.node);
        if !node.is_foldable() || !node.expanded {
            return;
        }
        let want_close = !row.close;
        let rows = self.rows();
        if let Some(i) = rows
            .iter()
            .position(|r| r.node == row.node && r.close == want_close)
        {
            self.set_focus(i as isize, view);
        }
    }

    // ----- horizontal --------------------------------------------------------

    pub fn scroll_value(&mut self, delta: isize) {
        self.xoff = (self.xoff as isize + delta).max(0) as usize;
    }

    // ----- search -------------------------------------------------------------

    /// Run a pattern over the document and jump to the `n`-th match from
    /// the focus in `direction`. Returns a status message.
    pub fn search(
        &mut self,
        pattern: Pattern,
        direction: Direction,
        n: usize,
        view: View,
    ) -> String {
        let matches: Vec<NodeId> = (0..self.doc.len() as NodeId)
            .filter(|&i| pattern.regex.is_match(&fmt::search_text(&self.doc, i)))
            .collect();
        if matches.is_empty() {
            let msg = format!("Pattern not found: {}", pattern.input);
            self.search = Some(Search {
                pattern,
                direction,
                matches,
                current: None,
                wrapped: false,
            });
            return msg;
        }
        self.search = Some(Search {
            pattern,
            direction,
            matches,
            current: None,
            wrapped: false,
        });
        self.next_match(n, true, view)
    }

    /// `n` (`same` = true) / `N` (`same` = false): the n-th match from the
    /// focused node in (or against) the search direction, wrapping around.
    pub fn next_match(&mut self, n: usize, same: bool, view: View) -> String {
        let focused = self.focused_node();
        let Some(search) = self.search.as_mut() else {
            return "No previous search".to_string();
        };
        if search.matches.is_empty() {
            return format!("Pattern not found: {}", search.pattern.input);
        }
        let forward = (search.direction == Direction::Forward) == same;
        let count = search.matches.len();
        let mut wrapped = false;
        // Start from the first match strictly past the focus in the
        // direction of travel, then step n-1 further.
        let mut idx = if forward {
            match search.matches.iter().position(|&m| m > focused) {
                Some(i) => i,
                None => {
                    wrapped = true;
                    0
                }
            }
        } else {
            match search.matches.iter().rposition(|&m| m < focused) {
                Some(i) => i,
                None => {
                    wrapped = true;
                    count - 1
                }
            }
        };
        let extra = n.saturating_sub(1) % count;
        if extra > 0 {
            if forward {
                if idx + extra >= count {
                    wrapped = true;
                }
                idx = (idx + extra) % count;
            } else {
                if idx < extra {
                    wrapped = true;
                }
                idx = (idx + count - extra) % count;
            }
        }
        search.current = Some(idx);
        search.wrapped = wrapped;
        let target = search.matches[idx];
        let msg = format!(
            "{}{}  [{}/{}]{}",
            search.direction.prompt(),
            search.pattern.input,
            idx + 1,
            count,
            if wrapped { "  W" } else { "" }
        );
        self.focus_node(target, view);
        msg
    }

    /// Is this node one of the current search's matches?
    pub fn is_match(&self, node: NodeId) -> bool {
        match &self.search {
            Some(s) => s.matches.binary_search(&node).is_ok(),
            None => false,
        }
    }

    // ----- reload ----------------------------------------------------------------

    /// Re-read the file and fold the result in, keeping the reader's place:
    /// expansions whose paths survive stay, the focused node is found again
    /// by path, else by its nearest surviving ancestor and, within that, the
    /// node nearest the old source line; the focus keeps its screen line.
    pub fn reload(&mut self, view: View) {
        if self.explorer.is_some() {
            self.explorer_refresh(view);
            return;
        }
        let Some(path) = self.path.clone() else {
            return;
        };
        let format = self.explicit_format;
        match load::load_path(&path, format) {
            Ok(loaded) => {
                self.apply(loaded, view);
                self.gone = false;
                self.error = None;
            }
            Err(err) => {
                self.gone = !path.exists();
                self.error = Some(err);
                if let Ok(src) = std::fs::read_to_string(&path) {
                    self.source = src;
                }
            }
        }
        self.stamp = Stamp::of(&path);
        self.generation += 1;
        self.reload_due = None;
    }

    /// Replace the document, re-anchoring the view (see [`Tab::reload`]).
    pub fn apply(&mut self, loaded: Loaded, view: View) {
        let old_focus = self.focused_row();
        let anchor_path = old_focus.map(|r| self.doc.path(r.node));
        let anchor_line = old_focus.and_then(|r| self.doc.line_of(r.node));
        let anchor_close = old_focus.map(|r| r.close).unwrap_or(false);
        let screen_line = self.focus.saturating_sub(self.scroll);

        // Expansion state, by path.
        let mut expanded: Vec<Vec<Key>> = Vec::new();
        let mut collapsed: Vec<Vec<Key>> = Vec::new();
        for (i, n) in self.doc.nodes.iter().enumerate() {
            if n.is_foldable() {
                if n.expanded {
                    expanded.push(self.doc.path(i as NodeId));
                } else {
                    collapsed.push(self.doc.path(i as NodeId));
                }
            }
        }

        let mut doc = loaded.doc;
        // A fresh document has everything expanded; apply the old folds
        // where their paths still resolve. Containers the old document did
        // not have keep the default (expanded).
        for p in &collapsed {
            if let Some(id) = doc.resolve(p) {
                doc.set_expanded(id, false);
            }
        }
        for p in &expanded {
            if let Some(id) = doc.resolve(p) {
                doc.set_expanded(id, true);
            }
        }

        // The anchor.
        let anchor = match &anchor_path {
            Some(p) => match doc.resolve(p) {
                Some(id) => id,
                None => {
                    let (_, ancestor) = doc.resolve_prefix(p);
                    match anchor_line.and_then(|l| doc.nearest_by_line(ancestor, l)) {
                        Some(near) => near,
                        None => ancestor,
                    }
                }
            },
            None => 0,
        };
        doc.reveal(anchor);

        // Search matches are recomputed against the new document.
        if let Some(s) = self.search.as_mut() {
            s.matches = (0..doc.len() as NodeId)
                .filter(|&i| s.pattern.regex.is_match(&fmt::search_text(&doc, i)))
                .collect();
            s.current = None;
        }

        self.doc = doc;
        self.format = loaded.format;
        self.source = loaded.source;
        self.rows_dirty = true;
        let rows = self.rows();
        let mut focus = row_of(rows, anchor).unwrap_or(0);
        if anchor_close {
            if let Some(i) = rows.iter().position(|r| r.node == anchor && r.close) {
                focus = i;
            }
        }
        self.focus = focus;
        self.scroll = self.focus.saturating_sub(screen_line);
        let n = self.rows.len();
        self.scroll = self.scroll.min(n.saturating_sub(view.height));
        self.follow(view);
    }

    /// Parse the current source as another format (`:format yaml`).
    pub fn reformat(&mut self, format: Format, view: View) -> Result<(), LoadError> {
        let loaded = load::load_str(self.source.clone(), format)?;
        self.explicit_format = Some(format);
        self.apply(loaded, view);
        self.error = None;
        Ok(())
    }

    /// Has the file changed since the last stamp? (`None` when there is no
    /// file or it cannot be read.)
    pub fn stamp_changed(&self) -> bool {
        if let Some(ex) = &self.explorer {
            return ex.changed();
        }
        match &self.path {
            Some(p) => Stamp::of(p) != self.stamp,
            None => false,
        }
    }

    // ----- explorer --------------------------------------------------------------

    /// List whatever the expanded directories need and rebuild if anything
    /// was read. True when the tree changed.
    pub fn explorer_sync(&mut self, view: View) -> bool {
        let Some(ex) = self.explorer.as_mut() else {
            return false;
        };
        let dirs: Vec<PathBuf> = self
            .doc
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.is_container() && n.expanded)
            .map(|(i, _)| ex.fs_path(&self.doc.path(i as NodeId)))
            .collect();
        let mut changed = false;
        for dir in dirs {
            changed |= ex.ensure_children_listed(&dir);
        }
        if changed {
            self.rebuild_explorer(view);
        }
        changed
    }

    /// Re-read every listed directory and rebuild, keeping the place. A
    /// directory that appeared under an expanded one is listed too, so it
    /// gets its preview rather than a placeholder.
    pub fn explorer_refresh(&mut self, view: View) {
        if let Some(ex) = self.explorer.as_mut() {
            ex.relist_all();
            self.rebuild_explorer(view);
            self.explorer_sync(view);
            self.generation += 1;
            self.reload_due = None;
        }
    }

    fn rebuild_explorer(&mut self, view: View) {
        let Some(ex) = self.explorer.as_ref() else {
            return;
        };
        let doc = ex.build();
        self.apply(
            Loaded {
                doc,
                format: Format::Text,
                source: String::new(),
            },
            view,
        );
    }

    /// Show or hide dot-files.
    pub fn set_show_hidden(&mut self, show: bool, view: View) {
        if let Some(ex) = self.explorer.as_mut() {
            if ex.show_hidden != show {
                ex.show_hidden = show;
                self.rebuild_explorer(view);
                self.explorer_sync(view);
            }
        }
    }

    /// Make `dir` the explorer's root. Listings already read are kept, the
    /// folds under the old root survive when it sits inside the new one,
    /// and the focus stays on the same entry.
    pub fn reroot(&mut self, dir: &Path, view: View) {
        let Some(old) = self.explorer.take() else {
            return;
        };
        let old_root = old.root.clone();
        let show_hidden = old.show_hidden;
        let mut ex = Explorer::open(dir, show_hidden);
        ex.adopt_listings(old);
        // The old root's place under the new one, as node-path keys.
        let prefix: Vec<Key> = old_root
            .strip_prefix(&ex.root)
            .map(|rel| {
                rel.iter()
                    .map(|s| Key::Name(s.to_string_lossy().as_ref().into()))
                    .collect()
            })
            .unwrap_or_default();
        let focused = self.focused_row().map(|r| self.doc.path(r.node));
        let expanded: Vec<Vec<Key>> = self
            .doc
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.is_foldable() && n.expanded)
            .map(|(i, _)| self.doc.path(i as NodeId))
            .collect();
        let mut doc = ex.build();
        let mut target: NodeId = 0;
        if !prefix.is_empty() && old_root.starts_with(&ex.root) {
            for p in expanded {
                let mut q = prefix.clone();
                q.extend(p);
                if let Some(id) = doc.resolve(&q) {
                    doc.set_expanded(id, true);
                }
            }
            for i in 1..=prefix.len() {
                if let Some(id) = doc.resolve(&prefix[..i]) {
                    doc.set_expanded(id, true);
                    target = id;
                }
            }
            if let Some(f) = focused {
                let mut q = prefix.clone();
                q.extend(f);
                if let Some(id) = doc.resolve(&q) {
                    target = id;
                }
            }
        }
        self.doc = doc;
        self.rows_dirty = true;
        self.path = Some(ex.root.clone());
        self.title = crate::explorer::title_of(&ex.root);
        self.stamp = Stamp::of(&ex.root);
        self.explorer = Some(ex);
        self.focus_node(target, view);
        self.explorer_sync(view);
    }

    pub fn watchable(&self) -> bool {
        self.path.is_some()
    }

    /// The source line of the focused node, if known.
    pub fn focused_line(&mut self) -> Option<(u32, u32)> {
        let node = self.focused_node();
        let n = self.doc.node(node);
        (n.line > 0).then_some((n.line, n.col))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reposition {
    Top,
    Center,
    Bottom,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::Kind;

    const SRC: &str = r#"{
  "a": 1,
  "b": [true, {"c": "x", "d": [1, 2]}, null],
  "e": {"f": {"g": 2}},
  "h": "last"
}"#;

    fn tab(src: &str) -> Tab {
        let loaded = load::load_str(src.to_string(), Format::Json).unwrap();
        Tab::new(1, "t.json".into(), None, loaded)
    }

    fn v() -> View {
        View::new(10, 2)
    }

    fn focused_path(t: &mut Tab) -> String {
        let n = t.focused_node();
        fmt::path_dot(&t.doc.path(n))
    }

    fn focused_expanded(t: &mut Tab) -> bool {
        let n = t.focused_node();
        t.doc.node(n).expanded
    }

    #[test]
    fn vertical_moves_and_folds() {
        let mut t = tab(SRC);
        assert_eq!(t.row_count(), 14);
        t.move_down(2, v());
        assert_eq!(focused_path(&mut t), ".b");
        t.move_right(v()); // expanded: step in
        assert_eq!(focused_path(&mut t), ".b[0]");
        t.move_left(v()); // leaf: to parent
        assert_eq!(focused_path(&mut t), ".b");
        t.move_left(v()); // expanded container: collapse
        assert!(!focused_expanded(&mut t));
        assert_eq!(t.row_count(), 14 - 7);
        t.move_right(v()); // collapsed: expand
        assert_eq!(t.row_count(), 14);
        t.move_down(1, v());
        t.move_down(1, v());
        assert_eq!(focused_path(&mut t), ".b[1]");
        t.focus_parent(v());
        assert_eq!(focused_path(&mut t), ".b");
        t.sibling(1, true, v());
        assert_eq!(focused_path(&mut t), ".e");
        t.sibling(5, true, v());
        assert_eq!(focused_path(&mut t), ".h");
        t.sibling(1, false, v());
        assert_eq!(focused_path(&mut t), ".e");
        t.first_sibling(v());
        assert_eq!(focused_path(&mut t), ".a");
        t.last_sibling(v());
        assert_eq!(focused_path(&mut t), ".h");
        t.top(v());
        assert_eq!(focused_path(&mut t), "");
        t.bottom(v());
        assert_eq!(focused_path(&mut t), ".h");
        t.goto_row(4, v());
        assert_eq!(focused_path(&mut t), ".b[0]");
    }

    #[test]
    fn depth_changes() {
        let mut t = tab(SRC);
        t.move_down(3, v()); // .b[0]
        t.depth_change(1, true, v());
        assert_eq!(focused_path(&mut t), ".b[1].c");
        t.depth_change(1, false, v());
        assert_eq!(focused_path(&mut t), ".b[1]");
        t.depth_change(1, false, v());
        assert_eq!(focused_path(&mut t), ".b");
    }

    #[test]
    fn sibling_folds() {
        let mut t = tab(SRC);
        t.move_down(1, v()); // .a
        t.fold_siblings(false, false, v());
        assert_eq!(focused_path(&mut t), ".a");
        assert_eq!(t.row_count(), 5);
        t.fold_siblings(true, true, v());
        assert_eq!(t.row_count(), 14);
        t.move_down(5, v()); // .b[1].d
        t.fold_siblings(false, true, v());
        assert!(!focused_expanded(&mut t));
        assert_eq!(focused_path(&mut t), ".b[1].d");
        t.toggle(v());
        assert!(focused_expanded(&mut t));
    }

    #[test]
    fn line_mode_rows() {
        let mut t = tab(SRC);
        t.toggle_mode(v());
        assert!(t.line_mode);
        assert_eq!(t.row_count(), 14 + 6); // closing rows for root, b, b[1], d, e, f
    }

    #[test]
    fn line_mode_closing_rows() {
        let mut t = tab(r#"{"a": [1, 2], "b": 3}"#);
        t.toggle_mode(v());
        // rows: { , "a": [ , 1 , 2 , ] , "b": 3 , }
        assert_eq!(t.row_count(), 7);
        t.move_down(1, v());
        t.matching_pair(v());
        assert!(t.focused_row().unwrap().close);
        assert_eq!(t.focus, 4);
        t.matching_pair(v());
        assert_eq!(t.focus, 1);
        t.move_down(3, v()); // the "]" row
        assert!(t.focused_row().unwrap().close);
        t.move_left(v());
        assert_eq!(t.focus, 1);
        t.bottom(v()); // "}" row
        assert!(t.focused_row().unwrap().close);
        t.toggle_mode(v()); // data mode: lands on the previous item
        assert!(!t.line_mode);
        assert_eq!(focused_path(&mut t), ".b");
    }

    #[test]
    fn scrolling() {
        let src = format!(
            "[{}]",
            (0..100)
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(",")
        );
        let mut t = tab(&src);
        let view = View::new(10, 2);
        t.move_down(20, view);
        assert_eq!(t.focus, 20);
        assert_eq!(t.scroll, 13); // focus at the bottom minus scrolloff
        t.reposition(Reposition::Center, view);
        assert_eq!(t.scroll, 15);
        t.reposition(Reposition::Top, view);
        assert_eq!(t.scroll, 18);
        t.reposition(Reposition::Bottom, view);
        assert_eq!(t.scroll, 13);
        t.scroll_by(5, view);
        assert_eq!(t.scroll, 18);
        assert_eq!(t.focus, 20);
        t.scroll_by(5, view);
        assert_eq!(t.scroll, 23);
        assert_eq!(t.focus, 25); // dragged to scroll + so
        t.page(1, true, view);
        assert_eq!(t.scroll, 33);
        t.jump(5, true, view);
        assert_eq!((t.scroll, t.focus), (38, 40));
        t.top(view);
        t.jump(5, false, view);
        assert_eq!((t.scroll, t.focus), (0, 0));
        t.bottom(view);
        assert_eq!(t.scroll, 91);
        t.jump(5, true, view);
        assert_eq!((t.scroll, t.focus), (91, 100));
        t.move_up(1, view);
        assert_eq!(t.xoff, 0);
    }

    #[test]
    fn searching() {
        let mut t = tab(SRC);
        let p = crate::search::compile("1").unwrap();
        let msg = t.search(p, Direction::Forward, 1, v());
        assert!(msg.contains("[1/2]"), "{msg}");
        assert_eq!(focused_path(&mut t), ".a");
        let msg = t.next_match(1, true, v());
        assert!(msg.contains("[2/2]"), "{msg}");
        assert_eq!(focused_path(&mut t), ".b[1].d[0]");
        let msg = t.next_match(1, true, v());
        assert!(msg.contains("[1/2]") && msg.contains('W'), "{msg}");
        let msg = t.next_match(1, false, v());
        assert!(msg.contains("[2/2]"), "{msg}");
        let n = t.focused_node();
        assert!(t.is_match(n));
        let none = crate::search::compile("zzz").unwrap();
        assert_eq!(
            t.search(none, Direction::Forward, 1, v()),
            "Pattern not found: zzz"
        );
        // A match inside a collapsed container is revealed.
        let mut t = tab(SRC);
        t.expand_to_depth(1, v());
        assert_eq!(t.row_count(), 5);
        t.search(
            crate::search::compile("\"g\"").unwrap(),
            Direction::Forward,
            1,
            v(),
        );
        assert_eq!(focused_path(&mut t), ".e.f.g");
        assert!(t.row_count() > 5, "the match's ancestors were expanded");
    }

    #[test]
    fn key_search() {
        let mut t = tab(r#"{"x": {"a": 1}, "a": {"x": "a"}}"#);
        let p = crate::search::key_pattern("a");
        t.search(p, Direction::Forward, 1, v());
        assert_eq!(focused_path(&mut t), ".x.a");
        t.next_match(1, true, v());
        assert_eq!(focused_path(&mut t), ".a");
        t.next_match(1, true, v());
        assert_eq!(focused_path(&mut t), ".x.a");
    }

    fn write_temp(name: &str, contents: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("aless-tab-tests-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(name);
        std::fs::write(&p, contents).unwrap();
        p
    }

    #[test]
    fn reload_keeps_place_by_path() {
        let p = write_temp("a.json", SRC);
        let view = View::new(5, 1);
        let mut t = Tab::open(1, &p, None);
        assert!(t.error.is_none());
        t.move_down(6, view); // .b[1].d
        t.move_left(view); // collapse d
        t.move_down(1, view); // .b[2]
        assert_eq!(focused_path(&mut t), ".b[2]");
        let (focus, scroll) = (t.focus, t.scroll);
        std::fs::write(
            &p,
            r#"{"new": 0, "a": 1, "b": [true, {"c": "y", "d": [1, 2, 3]}, null], "e": {"f": {"g": 2}}}"#,
        )
        .unwrap();
        t.reload(view);
        assert!(t.error.is_none());
        assert_eq!(focused_path(&mut t), ".b[2]");
        assert_eq!(t.focus - t.scroll, focus - scroll);
        let d = t
            .doc
            .resolve(&[Key::Name("b".into()), Key::Index(1), Key::Name("d".into())])
            .unwrap();
        assert!(!t.doc.node(d).expanded, "collapsed state survives by path");
        assert_eq!(t.generation, 1);
    }

    #[test]
    fn reload_falls_back_to_ancestor_and_line() {
        let p = write_temp(
            "b.json",
            "{\n  \"a\": 1,\n  \"b\": {\n    \"c\": 2,\n    \"d\": 3\n  }\n}\n",
        );
        let view = View::new(10, 1);
        let mut t = Tab::open(1, &p, None);
        t.move_down(4, view); // .b.d at line 5
        assert_eq!(focused_path(&mut t), ".b.d");
        // Rename d -> dd on the same line: the path dies, the line survives.
        std::fs::write(
            &p,
            "{\n  \"a\": 1,\n  \"b\": {\n    \"c\": 2,\n    \"dd\": 3\n  }\n}\n",
        )
        .unwrap();
        t.reload(view);
        assert_eq!(focused_path(&mut t), ".b.dd");
        // Remove the whole of b: fall to the ancestor (root) then nearest line.
        std::fs::write(&p, "{\n  \"a\": 1,\n  \"z\": 9\n}\n").unwrap();
        t.reload(view);
        assert_eq!(focused_path(&mut t), ".z");
    }

    #[test]
    fn reload_errors_keep_the_document() {
        let p = write_temp("c.json", r#"{"a": 1}"#);
        let view = View::new(10, 1);
        let mut t = Tab::open(1, &p, None);
        std::fs::write(&p, "{\"a\": ").unwrap();
        t.reload(view);
        assert!(t.error.is_some());
        assert_eq!(t.doc.node(1).kind, Kind::Number(1.0));
        assert!(!t.gone);
        std::fs::remove_file(&p).unwrap();
        t.reload(view);
        assert!(t.gone);
        assert_eq!(t.doc.len(), 2);
        std::fs::write(&p, r#"{"a": 2}"#).unwrap();
        t.reload(view);
        assert!(!t.gone && t.error.is_none());
        assert_eq!(t.doc.node(1).kind, Kind::Number(2.0));
    }

    #[test]
    fn explorer_refresh_lists_new_subdirectories() {
        let dir = std::env::temp_dir().join(format!("aless-tab-explorer-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("a")).unwrap();
        std::fs::write(dir.join("a").join("seed.txt"), "s").unwrap();
        let view = View::new(10, 1);
        let mut t = Tab::explore(1, &dir, false);
        // Expand the root's child `a`, then add a directory inside it.
        let a = t.doc.resolve(&[Key::Name("a".into())]).unwrap();
        t.focus_node(a, view);
        t.toggle(view);
        assert!(t.doc.node(a).expanded);
        t.explorer_sync(view);
        std::fs::create_dir_all(dir.join("a").join("fresh").join("inner")).unwrap();
        t.explorer_refresh(view);
        let ex = t.explorer.as_ref().unwrap();
        let fresh = ex.root.join("a").join("fresh");
        assert!(
            ex.is_listed(&fresh),
            "a new child of an expanded directory is listed"
        );
        let node = t
            .doc
            .resolve(&[Key::Name("a".into()), Key::Name("fresh".into())])
            .unwrap();
        let kids: Vec<&str> = t
            .doc
            .children(node)
            .map(|c| t.doc.node(c).key.name().unwrap())
            .collect();
        assert_eq!(kids, vec!["inner"], "no placeholder: {kids:?}");
    }

    #[test]
    fn failed_open_is_a_tab() {
        let p = write_temp("d.json", "{ nope");
        let t = Tab::open(7, &p, None);
        assert!(t.error.is_some());
        assert_eq!(t.doc.len(), 1);
        assert_eq!(t.source, "{ nope");
        let missing = Tab::open(8, Path::new("/definitely/not/here.json"), None);
        assert!(missing.gone && missing.error.is_some());
    }

    #[test]
    fn reformat() {
        let loaded = load::load_str("a: 1\nb: [x, y]\n".to_string(), Format::Yaml).unwrap();
        let mut t = Tab::new(1, "t.yaml".into(), None, loaded);
        assert_eq!(t.row_count(), 5);
        assert!(t.reformat(Format::Json, v()).is_err());
        assert_eq!(t.row_count(), 5, "a failed reformat keeps the document");
        t.reformat(Format::Text, v()).unwrap();
        assert_eq!(t.format, Format::Text);
        assert_eq!(t.row_count(), 3);
    }
}

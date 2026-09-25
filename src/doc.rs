//! The document model: a parsed value flattened into a pre-order arena of
//! nodes, plus the visible-row projection the viewer scrolls through.
//!
//! Every node knows its parent, its key within that parent, its depth, its
//! direct child count and the size of its subtree (itself included). In
//! pre-order the first child of node `i` is `i + 1`, and the next sibling
//! of a node `c` is `c + size(c)`, so the tree needs no child vectors.
//!
//! Containers carry an `expanded` flag. The visible rows are rebuilt from
//! those flags with [`Doc::rows`]; in line mode every expanded non-empty
//! container also yields a closing-bracket row, as jless does.

use std::sync::Arc;

use tabnas::Value;

pub type NodeId = u32;
pub const NO_NODE: NodeId = u32::MAX;

/// How a node is addressed from its parent.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    Root,
    Index(u32),
    Name(Arc<str>),
}

impl Key {
    pub fn name(&self) -> Option<&str> {
        match self {
            Key::Name(n) => Some(n),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Object,
    Array,
    Null,
    Bool(bool),
    Number(f64),
    Str(Arc<str>),
}

impl Kind {
    pub fn is_container(&self) -> bool {
        matches!(self, Kind::Object | Kind::Array)
    }
}

#[derive(Clone, Debug)]
pub struct Node {
    pub parent: NodeId,
    pub key: Key,
    pub kind: Kind,
    pub depth: u32,
    /// Subtree size, this node included.
    pub size: u32,
    /// Direct child count.
    pub children: u32,
    pub expanded: bool,
    /// 1-based source line, 0 when unknown.
    pub line: u32,
    /// 1-based source column, 0 when unknown.
    pub col: u32,
}

impl Node {
    pub fn is_container(&self) -> bool {
        self.kind.is_container()
    }

    /// A container with at least one child: the only kind of node that
    /// folds.
    pub fn is_foldable(&self) -> bool {
        self.is_container() && self.children > 0
    }
}

/// One visible line of the tree pane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    pub node: NodeId,
    /// A closing-bracket line (line mode only).
    pub close: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Doc {
    pub nodes: Vec<Node>,
}

impl Doc {
    /// Flatten a parsed value. Every container starts expanded.
    pub fn from_value(value: &Value) -> Doc {
        let mut nodes: Vec<Node> = Vec::new();
        // Explicit stack, so a deeply nested document cannot overflow the
        // call stack. Children are pushed in reverse so they pop in order.
        let mut stack: Vec<(NodeId, Key, &Value, u32)> = vec![(NO_NODE, Key::Root, value, 0)];
        while let Some((parent, key, v, depth)) = stack.pop() {
            let id = nodes.len() as NodeId;
            let (kind, children) = match v {
                Value::Object(m) => {
                    for (k, cv) in m.iter().rev() {
                        stack.push((id, Key::Name(Arc::from(k.as_str())), cv, depth + 1));
                    }
                    (Kind::Object, m.len())
                }
                Value::MapRef(m) => {
                    for (k, cv) in m.value.iter().rev() {
                        stack.push((id, Key::Name(Arc::from(k.as_str())), cv, depth + 1));
                    }
                    (Kind::Object, m.value.len())
                }
                Value::Array(a) => {
                    for (i, cv) in a.iter().enumerate().rev() {
                        stack.push((id, Key::Index(i as u32), cv, depth + 1));
                    }
                    (Kind::Array, a.len())
                }
                Value::ListRef(l) => {
                    for (i, cv) in l.value.iter().enumerate().rev() {
                        stack.push((id, Key::Index(i as u32), cv, depth + 1));
                    }
                    (Kind::Array, l.value.len())
                }
                Value::Null | Value::Undefined => (Kind::Null, 0),
                Value::Bool(b) => (Kind::Bool(*b), 0),
                Value::Number(n) => (Kind::Number(*n), 0),
                Value::String(s) => (Kind::Str(Arc::from(s.as_str())), 0),
                Value::Text(t) => (Kind::Str(Arc::from(t.string.as_str())), 0),
            };
            nodes.push(Node {
                parent,
                key,
                kind,
                depth,
                size: 1,
                children: children as u32,
                expanded: true,
                line: 0,
                col: 0,
            });
        }
        // Descendants have larger indices than their ancestors, so a reverse
        // sweep completes every subtree size before it is added to a parent.
        for i in (1..nodes.len()).rev() {
            let (size, parent) = (nodes[i].size, nodes[i].parent);
            nodes[parent as usize].size += size;
        }
        Doc { nodes }
    }

    /// A document from nodes already in pre-order with `parent`, `key`,
    /// `kind`, `depth`, `children` and `expanded` filled in; subtree sizes
    /// are computed here.
    pub fn from_preorder(mut nodes: Vec<Node>) -> Doc {
        for n in &mut nodes {
            n.size = 1;
        }
        for i in (1..nodes.len()).rev() {
            let (size, parent) = (nodes[i].size, nodes[i].parent);
            nodes[parent as usize].size += size;
        }
        Doc { nodes }
    }

    /// A document made of plain text lines (the fallback for unknown
    /// formats): an array of strings, each positioned on its own line.
    pub fn from_lines(lines: &[&str]) -> Doc {
        let mut nodes = Vec::with_capacity(lines.len() + 1);
        nodes.push(Node {
            parent: NO_NODE,
            key: Key::Root,
            kind: Kind::Array,
            depth: 0,
            size: lines.len() as u32 + 1,
            children: lines.len() as u32,
            expanded: true,
            line: if lines.is_empty() { 0 } else { 1 },
            col: 1,
        });
        for (i, l) in lines.iter().enumerate() {
            nodes.push(Node {
                parent: 0,
                key: Key::Index(i as u32),
                kind: Kind::Str(Arc::from(*l)),
                depth: 1,
                size: 1,
                children: 0,
                expanded: true,
                line: i as u32 + 1,
                col: 1,
            });
        }
        Doc { nodes }
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id as usize]
    }

    pub fn root(&self) -> &Node {
        &self.nodes[0]
    }

    /// First index past the subtree of `id`.
    pub fn subtree_end(&self, id: NodeId) -> NodeId {
        id + self.nodes[id as usize].size
    }

    pub fn parent(&self, id: NodeId) -> Option<NodeId> {
        let p = self.nodes[id as usize].parent;
        (p != NO_NODE).then_some(p)
    }

    /// Direct children of `id`, in order.
    pub fn children(&self, id: NodeId) -> Children<'_> {
        let n = &self.nodes[id as usize];
        Children {
            doc: self,
            next: id + 1,
            left: n.children,
        }
    }

    pub fn first_child(&self, id: NodeId) -> Option<NodeId> {
        (self.nodes[id as usize].children > 0).then_some(id + 1)
    }

    pub fn next_sibling(&self, id: NodeId) -> Option<NodeId> {
        let n = &self.nodes[id as usize];
        if n.parent == NO_NODE {
            return None;
        }
        let cand = id + n.size;
        (cand < self.nodes.len() as NodeId && self.nodes[cand as usize].parent == n.parent)
            .then_some(cand)
    }

    pub fn prev_sibling(&self, id: NodeId) -> Option<NodeId> {
        let parent = self.parent(id)?;
        let mut prev = None;
        for c in self.children(parent) {
            if c == id {
                return prev;
            }
            prev = Some(c);
        }
        None
    }

    pub fn first_sibling(&self, id: NodeId) -> NodeId {
        match self.parent(id) {
            Some(p) => p + 1,
            None => id,
        }
    }

    pub fn last_sibling(&self, id: NodeId) -> NodeId {
        match self.parent(id) {
            Some(p) => self.children(p).last().unwrap_or(id),
            None => id,
        }
    }

    /// Is `anc` a proper ancestor of `id`?
    pub fn is_ancestor(&self, anc: NodeId, id: NodeId) -> bool {
        anc < id && id < self.subtree_end(anc)
    }

    /// The path of keys from the root down to `id` (root excluded).
    pub fn path(&self, id: NodeId) -> Vec<Key> {
        let mut out = Vec::new();
        let mut cur = id;
        while let Some(p) = self.parent(cur) {
            out.push(self.nodes[cur as usize].key.clone());
            cur = p;
        }
        out.reverse();
        out
    }

    /// Resolve a path from the root; `None` when any segment is missing.
    pub fn resolve(&self, path: &[Key]) -> Option<NodeId> {
        let mut cur: NodeId = 0;
        for seg in path {
            cur = self.child_by_key(cur, seg)?;
        }
        Some(cur)
    }

    /// The longest prefix of `path` that resolves, and the node it reaches.
    pub fn resolve_prefix(&self, path: &[Key]) -> (usize, NodeId) {
        let mut cur: NodeId = 0;
        for (i, seg) in path.iter().enumerate() {
            match self.child_by_key(cur, seg) {
                Some(c) => cur = c,
                None => return (i, cur),
            }
        }
        (path.len(), cur)
    }

    pub fn child_by_key(&self, parent: NodeId, key: &Key) -> Option<NodeId> {
        match key {
            Key::Root => None,
            Key::Index(i) => {
                if !matches!(self.nodes[parent as usize].kind, Kind::Array) {
                    return None;
                }
                self.children(parent).nth(*i as usize)
            }
            Key::Name(n) => {
                if !matches!(self.nodes[parent as usize].kind, Kind::Object) {
                    return None;
                }
                self.children(parent)
                    .find(|&c| self.nodes[c as usize].key.name() == Some(n))
            }
        }
    }

    // ----- folding ---------------------------------------------------------

    pub fn set_expanded(&mut self, id: NodeId, expanded: bool) {
        let n = &mut self.nodes[id as usize];
        if n.is_foldable() {
            n.expanded = expanded;
        }
    }

    pub fn toggle(&mut self, id: NodeId) {
        let n = &mut self.nodes[id as usize];
        if n.is_foldable() {
            n.expanded = !n.expanded;
        }
    }

    /// Set the fold state of every foldable node in the subtree of `id`,
    /// `id` included.
    pub fn set_expanded_deep(&mut self, id: NodeId, expanded: bool) {
        let end = self.subtree_end(id);
        for i in id..end {
            let n = &mut self.nodes[i as usize];
            if n.is_foldable() {
                n.expanded = expanded;
            }
        }
    }

    /// Set the fold state of `id` and each of its siblings (shallow).
    pub fn set_siblings_expanded(&mut self, id: NodeId, expanded: bool) {
        match self.parent(id) {
            Some(p) => {
                let sibs: Vec<NodeId> = self.children(p).collect();
                for s in sibs {
                    self.set_expanded(s, expanded);
                }
            }
            None => self.set_expanded(id, expanded),
        }
    }

    /// Set the fold state of `id`, its siblings, and all their descendants.
    pub fn set_siblings_expanded_deep(&mut self, id: NodeId, expanded: bool) {
        match self.parent(id) {
            Some(p) => {
                let sibs: Vec<NodeId> = self.children(p).collect();
                for s in sibs {
                    self.set_expanded_deep(s, expanded);
                }
            }
            None => self.set_expanded_deep(id, expanded),
        }
    }

    /// Expand every ancestor of `id` so that it becomes visible.
    pub fn reveal(&mut self, id: NodeId) {
        let mut cur = id;
        while let Some(p) = self.parent(cur) {
            self.nodes[p as usize].expanded = true;
            cur = p;
        }
    }

    /// Collapse everything deeper than `depth` (containers at exactly
    /// `depth` stay expanded so their children show as collapsed rows).
    pub fn expand_to_depth(&mut self, depth: u32) {
        for n in &mut self.nodes {
            if n.is_foldable() {
                n.expanded = n.depth < depth;
            }
        }
    }

    /// Is `id` visible, i.e. every ancestor expanded?
    pub fn is_visible(&self, id: NodeId) -> bool {
        let mut cur = id;
        while let Some(p) = self.parent(cur) {
            if !self.nodes[p as usize].expanded {
                return false;
            }
            cur = p;
        }
        true
    }

    /// The nearest visible node on the path from the root to `id`: `id`
    /// itself when visible, else the outermost collapsed ancestor.
    pub fn first_visible_ancestor(&self, id: NodeId) -> NodeId {
        let mut best = id;
        let mut cur = id;
        while let Some(p) = self.parent(cur) {
            if !self.nodes[p as usize].expanded {
                best = p;
            }
            cur = p;
        }
        best
    }

    // ----- rows ------------------------------------------------------------

    /// The visible rows in document order. In line mode every expanded,
    /// non-empty container is followed (after its subtree) by a closing
    /// row.
    pub fn rows(&self, line_mode: bool) -> Vec<Row> {
        let n = self.nodes.len() as NodeId;
        let mut rows = Vec::with_capacity(self.nodes.len().min(1 << 20));
        let mut open: Vec<NodeId> = Vec::new();
        let mut i: NodeId = 0;
        while i < n {
            while let Some(&c) = open.last() {
                if i >= self.subtree_end(c) {
                    rows.push(Row {
                        node: c,
                        close: true,
                    });
                    open.pop();
                } else {
                    break;
                }
            }
            rows.push(Row {
                node: i,
                close: false,
            });
            let nd = &self.nodes[i as usize];
            if nd.is_foldable() {
                if nd.expanded {
                    if line_mode {
                        open.push(i);
                    }
                    i += 1;
                } else {
                    i += nd.size;
                }
            } else {
                i += 1;
            }
        }
        while let Some(c) = open.pop() {
            rows.push(Row {
                node: c,
                close: true,
            });
        }
        rows
    }

    // ----- provenance ------------------------------------------------------

    /// The source line of `id`, or the first known line in its subtree.
    pub fn line_of(&self, id: NodeId) -> Option<u32> {
        let end = self.subtree_end(id);
        (id..end)
            .map(|i| self.nodes[i as usize].line)
            .find(|&l| l != 0)
    }

    /// The node in the subtree of `root` whose known source line is nearest
    /// to `line`; ties prefer the earlier node. `None` when nothing in that
    /// subtree carries a position.
    pub fn nearest_by_line(&self, root: NodeId, line: u32) -> Option<NodeId> {
        let end = self.subtree_end(root);
        let mut best: Option<(u32, NodeId)> = None;
        for i in root..end {
            let l = self.nodes[i as usize].line;
            if l == 0 {
                continue;
            }
            let d = l.abs_diff(line);
            match best {
                Some((bd, _)) if bd <= d => {}
                _ => best = Some((d, i)),
            }
            if d == 0 {
                break;
            }
        }
        best.map(|(_, id)| id)
    }
}

pub struct Children<'a> {
    doc: &'a Doc,
    next: NodeId,
    left: u32,
}

impl Iterator for Children<'_> {
    type Item = NodeId;
    fn next(&mut self) -> Option<NodeId> {
        if self.left == 0 {
            return None;
        }
        let cur = self.next;
        self.left -= 1;
        self.next = self.doc.subtree_end(cur);
        Some(cur)
    }
}

/// Find the row whose open line shows `node` (`None` when hidden).
pub fn row_of(rows: &[Row], node: NodeId) -> Option<usize> {
    // Open rows are in ascending node order, so a binary search over them
    // would work, but close rows interleave; a scan is simple and rows are
    // rebuilt rarely.
    rows.iter().position(|r| r.node == node && !r.close)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(src: &str) -> Doc {
        Doc::from_value(&tabnas_json::parse(src).unwrap())
    }

    #[test]
    fn arena_shape() {
        let d = doc(r#"{"a": 1, "b": [true, null, {"c": "x"}], "d": {}}"#);
        assert_eq!(d.len(), 8);
        assert_eq!(d.root().size, 8);
        assert_eq!(d.root().children, 3);
        assert_eq!(d.node(1).key, Key::Name("a".into()));
        assert_eq!(d.node(2).key, Key::Name("b".into()));
        assert_eq!(d.node(2).size, 5);
        assert_eq!(d.node(3).key, Key::Index(0));
        assert_eq!(d.node(5).kind, Kind::Object);
        assert_eq!(d.node(6).kind, Kind::Str("x".into()));
        assert_eq!(d.node(7).key, Key::Name("d".into()));
        assert!(!d.node(7).is_foldable());
        assert_eq!(d.children(0).collect::<Vec<_>>(), vec![1, 2, 7]);
        assert_eq!(d.children(2).collect::<Vec<_>>(), vec![3, 4, 5]);
        assert_eq!(d.next_sibling(2), Some(7));
        assert_eq!(d.next_sibling(7), None);
        assert_eq!(d.prev_sibling(7), Some(2));
        assert_eq!(d.prev_sibling(1), None);
        assert_eq!(d.first_sibling(5), 3);
        assert_eq!(d.last_sibling(3), 5);
        assert!(d.is_ancestor(2, 6));
        assert!(!d.is_ancestor(1, 6));
    }

    #[test]
    fn paths_resolve() {
        let d = doc(r#"{"a": 1, "b": [true, null, {"c": "x"}]}"#);
        let p = d.path(6);
        assert_eq!(
            p,
            vec![Key::Name("b".into()), Key::Index(2), Key::Name("c".into())]
        );
        assert_eq!(d.resolve(&p), Some(6));
        assert_eq!(d.resolve(&[Key::Name("zz".into())]), None);
        let (n, id) = d.resolve_prefix(&[Key::Name("b".into()), Key::Index(9)]);
        assert_eq!((n, id), (1, 2));
        assert_eq!(d.resolve(&[]), Some(0));
    }

    #[test]
    fn rows_data_and_line_mode() {
        let mut d = doc(r#"{"a": 1, "b": [true, {"c": "x"}], "d": {}}"#);
        let rows = d.rows(false);
        assert_eq!(rows.len(), d.len());
        assert!(rows.iter().all(|r| !r.close));
        let rows = d.rows(true);
        // root open, a, b open, true, {c} open, c, close {c}, close b, d, close root
        let expect: Vec<(NodeId, bool)> = vec![
            (0, false),
            (1, false),
            (2, false),
            (3, false),
            (4, false),
            (5, false),
            (4, true),
            (2, true),
            (6, false),
            (0, true),
        ];
        assert_eq!(
            rows.iter().map(|r| (r.node, r.close)).collect::<Vec<_>>(),
            expect
        );
        d.set_expanded(2, false);
        let rows = d.rows(true);
        let expect: Vec<(NodeId, bool)> =
            vec![(0, false), (1, false), (2, false), (6, false), (0, true)];
        assert_eq!(
            rows.iter().map(|r| (r.node, r.close)).collect::<Vec<_>>(),
            expect
        );
        assert_eq!(row_of(&rows, 6), Some(3));
        assert_eq!(row_of(&rows, 3), None);
        assert!(!d.is_visible(3));
        assert_eq!(d.first_visible_ancestor(5), 2);
        d.reveal(5);
        assert!(d.is_visible(5));
    }

    #[test]
    fn folding_ops() {
        let mut d = doc(r#"[[1, [2]], [3], 4]"#);
        d.set_siblings_expanded_deep(1, false);
        assert!(!d.node(1).expanded && !d.node(3).expanded && !d.node(5).expanded);
        assert_eq!(d.rows(false).len(), 4);
        d.set_siblings_expanded(1, true);
        assert!(d.node(1).expanded && d.node(5).expanded && !d.node(3).expanded);
        d.expand_to_depth(1);
        assert!(d.node(0).expanded && !d.node(1).expanded);
        d.toggle(1);
        assert!(d.node(1).expanded);
        d.toggle(7); // a leaf: no-op
        assert!(d.node(7).expanded);
    }

    #[test]
    fn lines_doc() {
        let d = Doc::from_lines(&["a", "", "c"]);
        assert_eq!(d.len(), 4);
        assert_eq!(d.node(2).kind, Kind::Str("".into()));
        assert_eq!(d.node(3).line, 3);
        assert_eq!(d.line_of(0), Some(1));
        assert_eq!(d.nearest_by_line(0, 2), Some(2));
        assert_eq!(d.nearest_by_line(0, 99), Some(3));
    }

    #[test]
    fn deep_nesting_does_not_recurse() {
        // Build straight from a hand-made value: the parser has its own
        // depth policy, the arena must not.
        const DEPTH: usize = 4000;
        let mut v = Value::Array(Arc::new(vec![]));
        for _ in 0..DEPTH {
            v = Value::Array(Arc::new(vec![v]));
        }
        let d = Doc::from_value(&v);
        // Dropping the value would recurse DEPTH frames deep, which the
        // Windows test-thread stack does not have; the parsers cap nesting
        // long before this, so only this hand-made value is at risk.
        std::mem::forget(v);
        assert_eq!(d.len(), DEPTH + 1);
        assert_eq!(d.node(DEPTH as NodeId).depth, DEPTH as u32);
        assert_eq!(d.rows(true).len(), DEPTH + 1 + DEPTH);
        assert_eq!(d.rows(false).len(), DEPTH + 1);
    }
}

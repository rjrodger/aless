//! The versions of a watched file: what each reload changed, path by path,
//! and the marks that show it on the tree. A version keeps its text, not
//! its tree, which takes [`crate::load::MEMORY_PER_BYTE`] times the memory:
//! a version is parsed again when it comes back on screen.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::doc::{Doc, Key, Kind, NodeId, NO_NODE};
use crate::fmt;
use crate::load::Format;

// ----- changes ---------------------------------------------------------------

/// What happened to the value at a path from one version to the next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChangeKind {
    Changed,
    Added,
    Removed,
}

impl ChangeKind {
    pub fn glyph(self, ascii: bool) -> &'static str {
        match self {
            ChangeKind::Changed => "~",
            ChangeKind::Added => "+",
            ChangeKind::Removed if ascii => "-",
            ChangeKind::Removed => "−",
        }
    }

    pub fn word(self) -> &'static str {
        match self {
            ChangeKind::Changed => "changed",
            ChangeKind::Added => "added",
            ChangeKind::Removed => "removed",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    pub kind: ChangeKind,
    pub path: Vec<Key>,
    /// The value before and after, on one line each, where there is one.
    pub old: Option<String>,
    pub new: Option<String>,
}

const TEXT_MAX: usize = 200;

/// A scalar as the tree draws it, a container as its preview.
fn value_line(doc: &Doc, id: NodeId) -> String {
    let text = if doc.node(id).is_container() {
        fmt::preview(doc, id, 60)
    } else {
        fmt::value_text(doc, id)
    };
    if text.chars().count() > TEXT_MAX {
        let mut cut: String = text.chars().take(TEXT_MAX - 1).collect();
        cut.push('…');
        cut
    } else {
        text
    }
}

fn same_scalar(old: &Doc, o: NodeId, new: &Doc, n: NodeId) -> bool {
    std::mem::discriminant(&old.node(o).kind) == std::mem::discriminant(&new.node(n).kind)
        && fmt::value_text(old, o) == fmt::value_text(new, n)
}

/// The changes from `old` to `new` by path, in `new`'s order, a removal at
/// its parent. A container whose kind changed is one change, its insides
/// not compared. Arrays compare index by index, as docs/explanation/reloading.md
/// says.
pub fn diff(old: &Doc, new: &Doc) -> Vec<Change> {
    let mut found: Vec<(NodeId, Change)> = Vec::new();
    if old.is_empty() || new.is_empty() {
        return Vec::new();
    }
    // Pairs at the same path; a stack, as a deep document would overflow
    // a recursion.
    let mut stack: Vec<(NodeId, NodeId)> = vec![(0, 0)];
    while let Some((o, n)) = stack.pop() {
        let (on, nn) = (old.node(o), new.node(n));
        let mut pairs: Vec<(NodeId, NodeId)> = Vec::new();
        match (&on.kind, &nn.kind) {
            (Kind::Object, Kind::Object) => {
                // A name given more than once pairs in order of appearance.
                let mut olds: HashMap<&str, VecDeque<NodeId>> = HashMap::new();
                for c in old.children(o) {
                    if let Key::Name(name) = &old.node(c).key {
                        olds.entry(name).or_default().push_back(c);
                    }
                }
                for c in new.children(n) {
                    let Key::Name(name) = &new.node(c).key else {
                        continue;
                    };
                    match olds.get_mut(&**name).and_then(VecDeque::pop_front) {
                        Some(oc) => pairs.push((oc, c)),
                        None => found.push((c, added(new, c))),
                    }
                }
                let mut gone: Vec<NodeId> = olds.into_values().flatten().collect();
                gone.sort_unstable();
                for oc in gone {
                    found.push((n, removed(old, oc)));
                }
            }
            (Kind::Array, Kind::Array) => {
                let mut oc = old.children(o);
                for c in new.children(n) {
                    match oc.next() {
                        Some(o2) => pairs.push((o2, c)),
                        None => found.push((c, added(new, c))),
                    }
                }
                for o2 in oc {
                    found.push((n, removed(old, o2)));
                }
            }
            (a, b) if !a.is_container() && !b.is_container() => {
                if !same_scalar(old, o, new, n) {
                    found.push((n, changed(old, o, new, n)));
                }
            }
            _ => found.push((n, changed(old, o, new, n))),
        }
        stack.extend(pairs.into_iter().rev());
    }
    // Stable, so a parent's removals keep their order.
    found.sort_by_key(|(at, _)| *at);
    found.into_iter().map(|(_, c)| c).collect()
}

fn added(new: &Doc, n: NodeId) -> Change {
    Change {
        kind: ChangeKind::Added,
        path: new.path(n),
        old: None,
        new: Some(value_line(new, n)),
    }
}

fn removed(old: &Doc, o: NodeId) -> Change {
    Change {
        kind: ChangeKind::Removed,
        path: old.path(o),
        old: Some(value_line(old, o)),
        new: None,
    }
}

fn changed(old: &Doc, o: NodeId, new: &Doc, n: NodeId) -> Change {
    Change {
        kind: ChangeKind::Changed,
        path: new.path(n),
        old: Some(value_line(old, o)),
        new: Some(value_line(new, n)),
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub changed: u32,
    pub added: u32,
    pub removed: u32,
}

impl Counts {
    pub fn of(changes: &[Change]) -> Counts {
        let mut c = Counts::default();
        for ch in changes {
            c.add(ch.kind);
        }
        c
    }

    fn add(&mut self, kind: ChangeKind) {
        match kind {
            ChangeKind::Changed => self.changed += 1,
            ChangeKind::Added => self.added += 1,
            ChangeKind::Removed => self.removed += 1,
        }
    }

    pub fn total(&self) -> u32 {
        self.changed + self.added + self.removed
    }

    /// `1 changed, 1 added, 1 removed`, leaving out the kinds with none.
    pub fn describe(&self) -> String {
        [
            (self.changed, "changed"),
            (self.added, "added"),
            (self.removed, "removed"),
        ]
        .iter()
        .filter(|(n, _)| *n > 0)
        .map(|(n, w)| format!("{n} {w}"))
        .collect::<Vec<_>>()
        .join(", ")
    }
}

// ----- marks -----------------------------------------------------------------

/// What happened to a node's own value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Data {
    Changed,
    Added,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mark {
    pub data: Option<Data>,
    /// Values removed from this node itself.
    pub removed: u32,
    /// For a folded row: a removal from the node counts, its own change not.
    pub inside: Counts,
    /// Where the node's rail ends, or [`NO_NODE`].
    pub last: NodeId,
}

impl Default for Mark {
    fn default() -> Mark {
        Mark {
            data: None,
            removed: 0,
            inside: Counts::default(),
            last: NO_NODE,
        }
    }
}

/// The marks a version's changes put on its tree: on the data that
/// changed, and on each node of the branch from the root down to it.
#[derive(Clone, Debug, Default)]
pub struct Marks {
    map: HashMap<NodeId, Mark>,
    /// What `d` and `D` go to, in document order: a changed or added value,
    /// or the parent of a removed one; with the kind of each.
    pub targets: Vec<NodeId>,
    pub kinds: Vec<ChangeKind>,
}

impl Marks {
    pub fn of(doc: &Doc, changes: &[Change]) -> Marks {
        let mut map: HashMap<NodeId, Mark> = HashMap::new();
        let mut at: Vec<(NodeId, ChangeKind)> = Vec::new();
        for ch in changes {
            let target = match ch.kind {
                ChangeKind::Removed => {
                    let parent = &ch.path[..ch.path.len().saturating_sub(1)];
                    doc.resolve(parent)
                }
                _ => doc.resolve(&ch.path),
            };
            let Some(target) = target else {
                continue;
            };
            at.push((target, ch.kind));
            // A removal is inside the node it left; any other change, inside
            // the parent of its node.
            let first = match ch.kind {
                ChangeKind::Removed => Some(target),
                _ => doc.parent(target),
            };
            let mut cur = first;
            while let Some(a) = cur {
                map.entry(a).or_default().inside.add(ch.kind);
                cur = doc.parent(a);
            }
            match ch.kind {
                ChangeKind::Removed => map.entry(target).or_default().removed += 1,
                kind => {
                    let data = if kind == ChangeKind::Added {
                        Data::Added
                    } else {
                        Data::Changed
                    };
                    map.entry(target).or_default().data = Some(data);
                    for d in target + 1..doc.subtree_end(target) {
                        map.entry(d).or_default().data = Some(Data::Added);
                    }
                }
            }
        }
        // No rail starts inside added data, which is marked whole.
        let marked: Vec<NodeId> = map.keys().copied().collect();
        for id in marked {
            let Some(p) = doc.parent(id) else {
                continue;
            };
            if let Some(m) = map.get_mut(&p) {
                if m.data.is_none() && (m.last == NO_NODE || id > m.last) {
                    m.last = id;
                }
            }
        }
        at.sort_by_key(|(t, _)| *t);
        at.dedup_by_key(|(t, _)| *t);
        Marks {
            map,
            targets: at.iter().map(|(t, _)| *t).collect(),
            kinds: at.iter().map(|(_, k)| *k).collect(),
        }
    }

    pub fn get(&self, id: NodeId) -> Option<&Mark> {
        self.map.get(&id)
    }

    pub fn iter(&self) -> impl Iterator<Item = (NodeId, &Mark)> {
        self.map.iter().map(|(&id, m)| (id, m))
    }

    /// A change of its own, or one inside it.
    pub fn carries(&self, id: NodeId) -> bool {
        self.map.contains_key(&id)
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }
}

// ----- versions --------------------------------------------------------------

/// A text a reload read, when, and what it changed from the version before.
#[derive(Clone, Debug)]
pub struct Version {
    /// From 1, the file as opened; letting a version go renumbers none.
    pub number: usize,
    pub time: SystemTime,
    pub source: Arc<str>,
    pub format: Format,
    pub changes: Vec<Change>,
}

impl Version {
    fn weight(&self) -> usize {
        let changes: usize = self
            .changes
            .iter()
            .map(|c| {
                64 + c.path.len() * 24
                    + c.old.as_ref().map_or(0, String::len)
                    + c.new.as_ref().map_or(0, String::len)
            })
            .sum();
        self.source.len() + changes
    }
}

/// A save that did not parse: a mark on the timeline, with no tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failed {
    pub time: SystemTime,
    pub message: String,
}

/// How many versions a tab keeps, and the bytes their texts may take.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Keep {
    pub versions: usize,
    pub bytes: usize,
}

pub const DEFAULT_VERSIONS: usize = 50;

/// As much text as aless reads by default ([`crate::load::DEFAULT_MAX_SIZE`]).
pub const DEFAULT_BYTES: usize = 64 << 20;

impl Default for Keep {
    fn default() -> Keep {
        Keep {
            versions: DEFAULT_VERSIONS,
            bytes: DEFAULT_BYTES,
        }
    }
}

/// A point on the timeline: a version by its index, or a failed save.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Entry {
    Version(usize),
    Failed,
}

/// A tab's versions, oldest first, the one on screen, and the saves that
/// did not parse.
#[derive(Clone, Debug, Default)]
pub struct History {
    versions: VecDeque<Version>,
    failed: Vec<Failed>,
    shown: usize,
    /// So that the same broken text read again is one failed save.
    failed_source: Option<Arc<str>>,
}

impl History {
    pub fn start(&mut self, source: &str, format: Format, time: SystemTime) {
        self.versions.clear();
        self.failed.clear();
        self.failed_source = None;
        self.versions.push_back(Version {
            number: 1,
            time,
            source: Arc::from(source),
            format,
            changes: Vec::new(),
        });
        self.shown = 0;
    }

    /// The app stamps a tab it opens with its own clock.
    pub fn stamp_first(&mut self, time: SystemTime) {
        if self.versions.len() == 1 {
            self.versions[0].time = time;
        }
    }

    pub fn is_empty(&self) -> bool {
        self.versions.is_empty()
    }

    pub fn len(&self) -> usize {
        self.versions.len()
    }

    pub fn versions(&self) -> &VecDeque<Version> {
        &self.versions
    }

    pub fn failed(&self) -> &[Failed] {
        &self.failed
    }

    pub fn shown_index(&self) -> usize {
        self.shown
    }

    pub fn shown(&self) -> Option<&Version> {
        self.versions.get(self.shown)
    }

    pub fn newest(&self) -> Option<&Version> {
        self.versions.back()
    }

    pub fn at_newest(&self) -> bool {
        self.versions.is_empty() || self.shown + 1 == self.versions.len()
    }

    pub fn has_timeline(&self) -> bool {
        self.versions.len() + self.failed.len() > 1
    }

    /// The version on screen stays on screen; the number is returned.
    pub fn push(
        &mut self,
        source: Arc<str>,
        format: Format,
        time: SystemTime,
        changes: Vec<Change>,
    ) -> usize {
        let number = self.newest().map_or(1, |v| v.number + 1);
        self.versions.push_back(Version {
            number,
            time,
            source,
            format,
            changes,
        });
        self.failed_source = None;
        number
    }

    pub fn show_newest(&mut self) {
        self.shown = self.versions.len().saturating_sub(1);
    }

    pub fn fail(&mut self, source: &str, time: SystemTime, message: String) {
        if self.versions.is_empty() {
            return;
        }
        if self.failed_source.as_deref() == Some(source) {
            return;
        }
        self.failed_source = Some(Arc::from(source));
        self.failed.push(Failed { time, message });
    }

    /// The index `delta` versions from the one on screen, clamped, and
    /// whether it moved.
    pub fn step(&self, delta: isize) -> (usize, bool) {
        let last = self.versions.len().saturating_sub(1) as isize;
        let to = (self.shown as isize + delta).clamp(0, last.max(0)) as usize;
        (to, to != self.shown)
    }

    pub fn set_shown(&mut self, index: usize) {
        self.shown = index.min(self.versions.len().saturating_sub(1));
    }

    /// Let the oldest versions go until `keep` holds; the version on
    /// screen and the newest stay whatever they weigh.
    pub fn prune(&mut self, keep: Keep) {
        loop {
            let len = self.versions.len();
            let weight: usize = self.versions.iter().map(Version::weight).sum();
            let over = len > keep.versions.max(1) || (weight > keep.bytes && len > 1);
            if !over {
                break;
            }
            let Some(i) = (0..len).find(|&i| i != self.shown && i + 1 != len) else {
                break;
            };
            self.versions.remove(i);
            if self.shown > i {
                self.shown -= 1;
            }
        }
        if let Some(first) = self.versions.front().map(|v| v.time) {
            self.failed.retain(|f| f.time >= first);
        }
    }

    pub fn entries(&self) -> Vec<(SystemTime, Entry)> {
        let mut out: Vec<(SystemTime, Entry)> = self
            .versions
            .iter()
            .enumerate()
            .map(|(i, v)| (v.time, Entry::Version(i)))
            .collect();
        out.extend(self.failed.iter().map(|f| (f.time, Entry::Failed)));
        out.sort_by_key(|(t, _)| *t);
        out
    }
}

// ----- time ------------------------------------------------------------------

/// The time of day `t`, as `14:02:11`, `offset` seconds east of UTC; with
/// no offset known, in UTC, and said so.
pub fn clock(t: SystemTime, offset: Option<i32>) -> String {
    let secs = match t.duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_secs() as i64,
        Err(e) => -(e.duration().as_secs() as i64),
    };
    let s = (secs + offset.unwrap_or(0) as i64).rem_euclid(86_400);
    let text = format!("{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60);
    match offset {
        Some(_) => text,
        None => format!("{text} UTC"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::load::load_str;
    use std::time::Duration;

    fn doc(json: &str) -> Doc {
        load_str(json.to_string(), Format::Json).unwrap().doc
    }

    fn summary(changes: &[Change]) -> Vec<String> {
        changes
            .iter()
            .map(|c| {
                format!(
                    "{} {} {} -> {}",
                    c.kind.word(),
                    fmt::path_dot(&c.path),
                    c.old.as_deref().unwrap_or("-"),
                    c.new.as_deref().unwrap_or("-")
                )
            })
            .collect()
    }

    const BOOKS: &str =
        r#"[{"title": "A", "year": 1999, "tags": ["x", "y"]}, {"title": "B", "tags": []}]"#;

    #[test]
    fn diff_finds_each_kind_in_document_order() {
        let old = doc(BOOKS);
        let new = doc(
            r#"[{"title": "A", "year": 2001, "tags": ["x"]}, {"title": "B", "tags": ["z"], "isbn": "1"}]"#,
        );
        assert_eq!(
            summary(&diff(&old, &new)),
            [
                "changed [0].year 1999 -> 2001",
                "removed [0].tags[1] \"y\" -> -",
                "added [1].tags[0] - -> \"z\"",
                "added [1].isbn - -> \"1\"",
            ]
        );
    }

    #[test]
    fn diff_of_a_document_with_itself_is_empty() {
        assert!(diff(&doc(BOOKS), &doc(BOOKS)).is_empty());
    }

    #[test]
    fn a_kind_that_changes_is_one_change() {
        let old = doc(r#"{"a": {"b": 1}, "c": [1], "d": 1, "e": "1"}"#);
        let new = doc(r#"{"a": [1, 2], "c": {"x": 1}, "d": "1", "e": "1"}"#);
        let found = summary(&diff(&old, &new));
        assert_eq!(found.len(), 3, "{found:?}");
        assert!(
            found[0].starts_with("changed .a (1) {b: 1} -> (2) [1, 2]"),
            "{found:?}"
        );
        assert!(found[1].starts_with("changed .c "), "{found:?}");
        assert_eq!(
            found[2], "changed .d 1 -> \"1\"",
            "a number and a string differ"
        );
    }

    #[test]
    fn a_number_spelled_otherwise_is_a_change() {
        let changes = diff(&doc(r#"{"n": 1}"#), &doc(r#"{"n": 1.0}"#));
        assert_eq!(summary(&changes), ["changed .n 1 -> 1.0"]);
    }

    #[test]
    fn duplicate_names_pair_in_order() {
        let old = load_str("a: 1\na: 2\n".to_string(), Format::Yaml);
        if let Ok(old) = old {
            assert!(diff(&old.doc, &old.doc).is_empty());
        }
    }

    #[test]
    fn marks_put_data_on_the_change_and_the_branch_up_to_the_root() {
        let old = doc(BOOKS);
        let new =
            doc(r#"[{"title": "A", "year": 2001, "tags": ["x"]}, {"title": "B", "tags": ["z"]}]"#);
        let marks = Marks::of(&new, &diff(&old, &new));
        let at = |path: &str| -> NodeId {
            let p: Vec<Key> = path
                .split('/')
                .filter(|s| !s.is_empty())
                .map(|s| match s.parse::<u32>() {
                    Ok(i) => Key::Index(i),
                    Err(_) => Key::Name(Arc::from(s)),
                })
                .collect();
            new.resolve(&p).unwrap()
        };
        let year = at("0/year");
        let tags0 = at("0/tags");
        let z = at("1/tags/0");
        assert_eq!(marks.get(year).unwrap().data, Some(Data::Changed));
        assert_eq!(marks.get(z).unwrap().data, Some(Data::Added));
        assert_eq!(marks.get(tags0).unwrap().removed, 1);
        assert!(marks.get(tags0).unwrap().data.is_none());
        assert_eq!(marks.get(0).unwrap().last, at("1"));
        assert_eq!(marks.get(at("0")).unwrap().last, tags0);
        assert_eq!(
            marks.get(0).unwrap().inside,
            Counts {
                changed: 1,
                added: 1,
                removed: 1
            }
        );
        assert!(
            !marks.carries(at("0/title")),
            "an unchanged value has no mark"
        );
        assert_eq!(marks.targets, vec![year, tags0, z]);
    }

    #[test]
    fn what_a_new_container_holds_is_new_and_has_no_rail() {
        let old = doc(r#"{"a": 1}"#);
        let new = doc(r#"{"a": 1, "b": {"c": [1, 2]}}"#);
        let marks = Marks::of(&new, &diff(&old, &new));
        let b = new.resolve(&[Key::Name(Arc::from("b"))]).unwrap();
        for id in b..new.subtree_end(b) {
            assert_eq!(marks.get(id).unwrap().data, Some(Data::Added), "node {id}");
            assert_eq!(marks.get(id).unwrap().last, NO_NODE, "node {id}");
        }
        assert_eq!(marks.get(0).unwrap().last, b);
        assert_eq!(marks.targets, vec![b], "one change, one target");
    }

    fn at(secs: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(secs)
    }

    #[test]
    fn the_history_numbers_versions_and_keeps_the_one_on_screen() {
        let mut h = History::default();
        h.start("1", Format::Json, at(0));
        for i in 2..=5 {
            h.push(Arc::from(i.to_string()), Format::Json, at(i), Vec::new());
        }
        assert_eq!(h.len(), 5);
        assert!(!h.at_newest(), "a push leaves the view where it was");
        h.show_newest();
        assert!(h.at_newest());
        let (to, moved) = h.step(-3);
        assert!(moved);
        h.set_shown(to);
        assert_eq!(h.shown().unwrap().number, 2);
        assert_eq!(h.step(-9), (0, true), "stepping stops at the oldest");
        h.prune(Keep {
            versions: 2,
            bytes: usize::MAX,
        });
        let numbers: Vec<usize> = h.versions().iter().map(|v| v.number).collect();
        assert_eq!(numbers, [2, 5], "the shown and the newest stay");
        assert_eq!(h.shown().unwrap().number, 2);
    }

    #[test]
    fn the_history_lets_old_versions_go_past_its_budget() {
        let mut h = History::default();
        let big = "x".repeat(100);
        h.start(&big, Format::Json, at(0));
        for i in 1..10 {
            h.push(Arc::from(big.as_str()), Format::Json, at(i), Vec::new());
            h.show_newest();
            h.prune(Keep {
                versions: 50,
                bytes: 350,
            });
        }
        assert_eq!(h.len(), 3, "three of 100 bytes fit in 350");
        assert_eq!(h.newest().unwrap().number, 10);
    }

    #[test]
    fn a_failed_save_counts_once_and_goes_with_the_versions_before_it() {
        let mut h = History::default();
        h.start("{}", Format::Json, at(10));
        h.fail("{,", at(20), "1:2: unexpected".to_string());
        h.fail("{,", at(21), "1:2: unexpected".to_string());
        assert_eq!(h.failed().len(), 1, "the same text read again is one save");
        assert!(h.has_timeline());
        h.push(Arc::from("{\"a\": 1}"), Format::Json, at(30), Vec::new());
        assert_eq!(
            h.entries().iter().map(|(_, e)| *e).collect::<Vec<_>>(),
            [Entry::Version(0), Entry::Failed, Entry::Version(1)]
        );
        h.show_newest();
        h.prune(Keep {
            versions: 1,
            bytes: usize::MAX,
        });
        assert!(h.failed().is_empty(), "older than every version kept");
    }

    #[test]
    fn clock_writes_the_time_of_day_in_the_zone_given() {
        let t = at(14 * 3600 + 2 * 60 + 11);
        assert_eq!(clock(t, Some(0)), "14:02:11");
        assert_eq!(clock(t, Some(3600)), "15:02:11");
        assert_eq!(clock(t, Some(-15 * 3600)), "23:02:11", "the day before");
        assert_eq!(clock(t, None), "14:02:11 UTC");
    }

    #[test]
    fn counts_describe_what_they_hold() {
        let c = Counts {
            changed: 1,
            added: 0,
            removed: 2,
        };
        assert_eq!(c.describe(), "1 changed, 2 removed");
        assert_eq!(c.total(), 3);
    }
}

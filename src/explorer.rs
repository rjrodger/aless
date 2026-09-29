//! The file explorer: a directory tree shown through the same machinery as
//! a document. Directories are containers, listed lazily as they are
//! expanded and one level ahead of that (so a collapsed directory's
//! preview can show its count and first names); files are leaves showing
//! size and format. `Enter` on a file opens it in a new tab.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use crate::doc::{Doc, Key, Kind, Node, NodeId, NO_NODE};
use crate::load::Format;

/// Directories listed at most, per explorer, so a deep expansion of a
/// huge tree stays bounded.
pub const MAX_LISTED: usize = 2000;

/// How long after a directory's last change another change can leave its
/// modification time as it was. A file system's clock ticks as coarsely
/// as two seconds (FAT), a second (ext3, HFS+) or the system timer's
/// 16 ms or so (NTFS), and two changes within one tick get one time.
const RACY: Duration = Duration::from_secs(2);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EntryKind {
    Dir,
    File,
    /// A symbolic link, with its target; never followed.
    Symlink(String),
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The name as shown and used as the node key (lossy for a name that
    /// is not valid UTF-8).
    pub name: String,
    /// The name as the filesystem has it, for every path operation.
    pub os_name: OsString,
    pub kind: EntryKind,
    pub size: u64,
    /// The format the file's extension or whole name implies, when a
    /// grammar handles it.
    pub format: Option<Format>,
}

impl Entry {
    pub fn is_dir(&self) -> bool {
        self.kind == EntryKind::Dir
    }

    /// The leaf text of a non-directory entry.
    pub fn describe(&self) -> String {
        match &self.kind {
            EntryKind::Dir => String::new(),
            EntryKind::File => match self.format {
                Some(f) => format!("{}  {}", human_size(self.size), f.name()),
                None => human_size(self.size),
            },
            EntryKind::Symlink(target) => format!("→ {target}"),
            EntryKind::Other => "(special file)".to_string(),
        }
    }
}

/// `273 B`, `1.2 KB`, `40 MB`, `3.1 GB`.
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if value >= 10.0 {
        format!("{value:.0} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[derive(Clone, Debug)]
pub struct Listing {
    pub entries: Vec<Entry>,
    pub mtime: Option<SystemTime>,
    pub error: Option<String>,
    /// Until when, by this machine's own steady clock, the listing is read
    /// again and compared rather than taken at its time's word: `None` for
    /// a directory read long enough after its last change.
    pub racy_until: Option<Instant>,
}

impl Listing {
    /// Was the directory read so soon after its last change that another
    /// change may yet leave its time as it was, and is that still so?
    fn racy(&self) -> bool {
        self.racy_until.is_some_and(|until| Instant::now() < until)
    }
}

/// How long a directory whose time is `mtime`, read at `read_at` by the
/// file system's clock, is to be read again from `now` on: what is left of
/// [`RACY`] since its last change, or all of it when the change is dated
/// ahead of the reading, by a skewed clock or a restored archive. Timed by
/// the steady clock from the first reading on, so a date ahead ends the
/// window as any other does, not when the clocks meet.
fn racy_until(mtime: Option<SystemTime>, read_at: SystemTime, now: Instant) -> Option<Instant> {
    let left = match read_at.duration_since(mtime?) {
        Ok(age) => RACY.checked_sub(age).filter(|left| !left.is_zero())?,
        Err(_) => RACY,
    };
    now.checked_add(left)
}

#[derive(Clone, Debug)]
pub struct Explorer {
    pub root: PathBuf,
    pub show_hidden: bool,
    /// Listed directories, by absolute path.
    listed: HashMap<PathBuf, Listing>,
    /// Listing stopped at [`MAX_LISTED`] directories.
    pub capped: bool,
}

fn read_listing(dir: &Path) -> Listing {
    let (read_at, now) = (SystemTime::now(), Instant::now());
    let mtime = std::fs::metadata(dir).ok().and_then(|m| m.modified().ok());
    let racy_until = racy_until(mtime, read_at, now);
    match std::fs::read_dir(dir) {
        Ok(read) => {
            let mut entries: Vec<Entry> = read
                .flatten()
                .map(|e| {
                    let os_name = e.file_name();
                    let name = os_name.to_string_lossy().into_owned();
                    let file_type = e.file_type().ok();
                    let is = |f: fn(&std::fs::FileType) -> bool| file_type.as_ref().is_some_and(f);
                    let (kind, size) = if is(std::fs::FileType::is_symlink) {
                        let target = std::fs::read_link(e.path())
                            .map(|p| p.display().to_string())
                            .unwrap_or_default();
                        (EntryKind::Symlink(target), 0)
                    } else if is(std::fs::FileType::is_dir) {
                        (EntryKind::Dir, 0)
                    } else if is(std::fs::FileType::is_file) {
                        (EntryKind::File, e.metadata().map(|m| m.len()).unwrap_or(0))
                    } else {
                        (EntryKind::Other, 0)
                    };
                    let format = (kind == EntryKind::File)
                        .then(|| Format::detect_known(Path::new(&name)))
                        .flatten();
                    Entry {
                        name,
                        os_name,
                        kind,
                        size,
                        format,
                    }
                })
                .collect();
            // Directories first, then names, case-insensitively.
            entries.sort_by(|a, b| {
                b.is_dir()
                    .cmp(&a.is_dir())
                    .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                    .then_with(|| a.name.cmp(&b.name))
            });
            Listing {
                entries,
                mtime,
                error: None,
                racy_until,
            }
        }
        Err(e) => Listing {
            entries: Vec::new(),
            mtime,
            error: Some(e.to_string()),
            racy_until,
        },
    }
}

/// Strip the Windows verbatim prefix `canonicalize` produces (`\\?\C:\x`
/// becomes `C:\x`, `\\?\UNC\srv\share` becomes `\\srv\share`), so paths
/// read, copy and join the way a user writes them. Other paths pass
/// through unchanged.
pub fn simplify(path: PathBuf) -> PathBuf {
    let Some(text) = path.to_str() else {
        return path;
    };
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    if let Some(rest) = text.strip_prefix(r"\\?\") {
        return PathBuf::from(rest);
    }
    path
}

/// The tab title for a directory: its name with a slash, or the path
/// itself at a filesystem root.
pub fn title_of(dir: &Path) -> String {
    match dir.file_name() {
        Some(n) => format!("{}/", n.to_string_lossy()),
        None => dir.display().to_string(),
    }
}

impl Explorer {
    /// Open a directory: it and its immediate subdirectories are listed.
    pub fn open(dir: &Path, show_hidden: bool) -> Explorer {
        let root = std::fs::canonicalize(dir)
            .map(simplify)
            .unwrap_or_else(|_| dir.to_path_buf());
        let mut ex = Explorer {
            root: root.clone(),
            show_hidden,
            listed: HashMap::new(),
            capped: false,
        };
        ex.ensure_children_listed(&root);
        ex
    }

    pub fn is_listed(&self, dir: &Path) -> bool {
        self.listed.contains_key(dir)
    }

    pub fn listing(&self, dir: &Path) -> Option<&Listing> {
        self.listed.get(dir)
    }

    /// Take over another explorer's listings that lie under this root (a
    /// re-root keeps what was already read and can still be shown; the
    /// rest would only cost cap and spurious refreshes).
    pub fn adopt_listings(&mut self, other: Explorer) {
        for (dir, listing) in other.listed {
            if dir.starts_with(&self.root) {
                self.listed.entry(dir).or_insert(listing);
            }
        }
    }

    /// List `dir` unless it already is. True when newly listed.
    fn list(&mut self, dir: &Path) -> bool {
        if self.listed.contains_key(dir) {
            return false;
        }
        if self.listed.len() >= MAX_LISTED {
            self.capped = true;
            return false;
        }
        self.listed.insert(dir.to_path_buf(), read_listing(dir));
        true
    }

    /// List `dir` and its subdirectories, so every child a viewer can see
    /// after expanding `dir` has a preview. True when anything was read.
    pub fn ensure_children_listed(&mut self, dir: &Path) -> bool {
        let mut changed = self.list(dir);
        let subdirs: Vec<PathBuf> = self
            .listed
            .get(dir)
            .map(|l| {
                l.entries
                    .iter()
                    .filter(|e| e.is_dir() && self.visible(e))
                    .map(|e| dir.join(&e.name))
                    .collect()
            })
            .unwrap_or_default();
        for sub in subdirs {
            changed |= self.list(&sub);
        }
        changed
    }

    /// Re-read every listed directory.
    pub fn relist_all(&mut self) {
        let dirs: Vec<PathBuf> = self.listed.keys().cloned().collect();
        for dir in dirs {
            self.listed.insert(dir.clone(), read_listing(&dir));
        }
    }

    /// Has any listed directory changed on disk since it was read? Its
    /// modification time answers for a directory read [`RACY`] or more
    /// after its last change. One read sooner is read again and compared,
    /// since a change within its file system's clock tick leaves the time
    /// as it was. A reading that finds nothing new takes the old one's
    /// place and keeps its window, timed by the steady clock from the
    /// first reading, so the directory is read again for [`RACY`] at most
    /// after its change is first seen, whatever its date.
    pub fn changed(&mut self) -> bool {
        for (dir, listing) in &mut self.listed {
            let mtime = std::fs::metadata(dir).ok().and_then(|m| m.modified().ok());
            if mtime != listing.mtime {
                return true;
            }
            if listing.racy() {
                let mut again = read_listing(dir);
                if again.entries != listing.entries || again.error != listing.error {
                    return true;
                }
                if again.mtime == listing.mtime {
                    // The same change: its window runs from the first reading.
                    again.racy_until = listing.racy_until;
                }
                *listing = again;
            }
        }
        false
    }

    fn visible(&self, entry: &Entry) -> bool {
        self.show_hidden || !entry.name.starts_with('.')
    }

    /// The filesystem path of a node path. Keys are the displayed names;
    /// each is mapped back to the name the filesystem has through the
    /// listing it came from, so a name that is not valid UTF-8 still
    /// resolves. A segment no listing knows is used as typed.
    pub fn fs_path(&self, path: &[Key]) -> PathBuf {
        let mut out = self.root.clone();
        for k in path {
            if let Key::Name(n) = k {
                let os_name = self
                    .listed
                    .get(&out)
                    .and_then(|l| l.entries.iter().find(|e| e.name == **n))
                    .map(|e| e.os_name.clone());
                match os_name {
                    Some(name) => out.push(name),
                    None => out.push(&**n),
                }
            }
        }
        out
    }

    /// The entry a node path names, if it is a listed directory's entry.
    pub fn entry(&self, path: &[Key]) -> Option<&Entry> {
        let (last, parents) = path.split_last()?;
        let name = last.name()?;
        let dir = self.fs_path(parents);
        self.listed
            .get(&dir)?
            .entries
            .iter()
            .find(|e| e.name == name)
    }

    /// The whole listed tree as a document: the root expanded, every other
    /// directory collapsed (the tab restores the folds it had).
    pub fn build(&self) -> Doc {
        let mut nodes: Vec<Node> = Vec::new();
        self.push_dir(&mut nodes, &self.root.clone(), NO_NODE, Key::Root, 0, true);
        Doc::from_preorder(nodes)
    }

    fn push_dir(
        &self,
        nodes: &mut Vec<Node>,
        dir: &Path,
        parent: NodeId,
        key: Key,
        depth: u32,
        expanded: bool,
    ) {
        let id = nodes.len() as NodeId;
        nodes.push(Node {
            parent,
            key,
            kind: Kind::Object,
            depth,
            size: 1,
            children: 0,
            expanded,
            line: 0,
            col: 0,
        });
        let leaf = |parent: NodeId, name: &str, text: String| Node {
            parent,
            key: Key::Name(Arc::from(name)),
            kind: Kind::Str(Arc::from(text.as_str())),
            depth: depth + 1,
            size: 1,
            children: 0,
            expanded: true,
            line: 0,
            col: 0,
        };
        let Some(listing) = self.listed.get(dir) else {
            // Not read yet: one placeholder keeps the directory foldable, so
            // expanding it lists it.
            nodes.push(leaf(id, "…", "(not listed yet)".to_string()));
            nodes[id as usize].children = 1;
            return;
        };
        let mut count = 0;
        if let Some(err) = &listing.error {
            nodes.push(leaf(id, "(error)", err.clone()));
            count += 1;
        }
        for e in listing.entries.iter().filter(|e| self.visible(e)) {
            count += 1;
            if e.is_dir() {
                self.push_dir(
                    nodes,
                    &dir.join(&e.name),
                    id,
                    Key::Name(Arc::from(e.name.as_str())),
                    depth + 1,
                    false,
                );
            } else {
                nodes.push(leaf(id, &e.name, e.describe()));
            }
        }
        nodes[id as usize].children = count;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("aless-explorer-{}-{}", std::process::id(), name));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub/deep")).unwrap();
        std::fs::write(dir.join("a.json"), "{\"a\": 1}").unwrap();
        std::fs::write(dir.join("b.yaml"), "b: 2\n").unwrap();
        std::fs::write(dir.join(".hidden"), "x").unwrap();
        std::fs::write(dir.join("sub/c.toml"), "c = 3\n").unwrap();
        std::fs::write(dir.join("sub/deep/x.txt"), "x\n").unwrap();
        std::fs::write(dir.join("notes"), "no extension").unwrap();
        dir
    }

    #[test]
    fn lists_one_level_ahead() {
        let dir = tree("ahead");
        let ex = Explorer::open(&dir, false);
        assert!(ex.is_listed(&ex.root));
        assert!(
            ex.is_listed(&ex.root.join("sub")),
            "children are listed for their previews"
        );
        assert!(
            !ex.is_listed(&ex.root.join("sub").join("deep")),
            "grandchildren wait"
        );
        let doc = ex.build();
        let names: Vec<String> = doc
            .children(0)
            .map(|c| doc.node(c).key.name().unwrap().to_string())
            .collect();
        assert_eq!(names, vec!["sub", "a.json", "b.yaml", "notes"]);
        let sub = doc.resolve(&[Key::Name("sub".into())]).unwrap();
        assert!(!doc.node(sub).expanded && doc.node(sub).children == 2);
        assert!(doc.root().expanded);
        let deep = doc
            .resolve(&[Key::Name("sub".into()), Key::Name("deep".into())])
            .unwrap();
        assert_eq!(
            doc.node(deep).children,
            1,
            "unlisted: a placeholder keeps it foldable"
        );
        let a = doc.resolve(&[Key::Name("a.json".into())]).unwrap();
        assert_eq!(
            &*match &doc.node(a).kind {
                Kind::Str(s) => s.clone(),
                _ => panic!(),
            },
            "8 B  json"
        );
    }

    #[test]
    fn hidden_entries_and_sync() {
        let dir = tree("hidden");
        let mut ex = Explorer::open(&dir, true);
        let doc = ex.build();
        assert_eq!(doc.root().children, 5);
        ex.show_hidden = false;
        assert_eq!(ex.build().root().children, 4);
        // Expanding sub lists deep.
        assert!(ex.ensure_children_listed(&ex.root.join("sub")));
        assert!(ex.is_listed(&ex.root.join("sub").join("deep")));
        assert!(
            !ex.ensure_children_listed(&ex.root.join("sub")),
            "nothing new the second time"
        );
        let doc = ex.build();
        let x = doc
            .resolve(&[
                Key::Name("sub".into()),
                Key::Name("deep".into()),
                Key::Name("x.txt".into()),
            ])
            .unwrap();
        assert_eq!(doc.node(x).depth, 3);
        let e = ex.entry(&doc.path(x)).unwrap();
        assert_eq!(e.kind, EntryKind::File);
        assert_eq!(e.format, Some(Format::Text));
        assert_eq!(
            ex.fs_path(&doc.path(x)),
            ex.root.join("sub").join("deep").join("x.txt")
        );
        assert!(ex.entry(&[Key::Name("nope".into())]).is_none());
    }

    #[test]
    fn detects_changes_and_relists() {
        let dir = tree("changes");
        let mut ex = Explorer::open(&dir, false);
        assert!(!ex.changed());
        std::thread::sleep(std::time::Duration::from_millis(30));
        std::fs::write(dir.join("new.csv"), "a,b\n").unwrap();
        // A coarse filesystem clock may not move the directory mtime; the
        // relist must show the file either way.
        ex.relist_all();
        let doc = ex.build();
        assert!(doc.resolve(&[Key::Name("new.csv".into())]).is_some());
        assert!(!ex.changed());
    }

    /// A change within the tick of the file system's clock that gave the
    /// directory its time leaves the time as it was. A listing read that
    /// soon after a change is read again and compared, so the change is
    /// seen all the same; one read after the window is taken at its
    /// time's word, and not read again.
    #[test]
    fn a_change_the_clock_does_not_show_is_still_seen() {
        let dir = tree("racy");
        let mut ex = Explorer::open(&dir, false);
        let root = ex.root.clone();
        assert!(ex.listed[&root].racy(), "read just after the tree was made");
        assert!(!ex.changed(), "nothing new");
        std::fs::write(dir.join("new.csv"), "a,b\n").unwrap();
        // As a coarse clock would leave it: the time the listing has.
        let now = std::fs::metadata(&root).unwrap().modified().unwrap();
        ex.listed.get_mut(&root).unwrap().mtime = Some(now);
        assert!(ex.changed(), "read again and compared");
        ex.relist_all();
        // Read again and found as it was, it keeps its first window.
        let window = ex.listed[&root].racy_until;
        assert!(window.is_some());
        assert!(!ex.changed());
        assert_eq!(ex.listed[&root].racy_until, window, "the window kept");
        // Past its window, it is taken at its time's word.
        ex.listed.get_mut(&root).unwrap().racy_until = Some(Instant::now());
        assert!(!ex.listed[&root].racy(), "not read again");
        assert!(!ex.changed());
    }

    /// The window to read a directory again is what is left of [`RACY`]
    /// since its change, none past it, and all of it, no more, for a
    /// change dated ahead of the reading: timed by the steady clock, it
    /// ends when it would for any change, not when the clocks meet.
    #[test]
    fn the_window_to_read_again_is_timed_here() {
        let (read_at, now) = (SystemTime::now(), Instant::now());
        let before = |secs: f64| Some(read_at - Duration::from_secs_f64(secs));
        assert_eq!(racy_until(None, read_at, now), None, "no time, no window");
        assert_eq!(racy_until(before(3.0), read_at, now), None, "long since");
        assert_eq!(racy_until(before(2.0), read_at, now), None, "just past");
        assert_eq!(
            racy_until(before(0.5), read_at, now),
            Some(now + Duration::from_millis(1500)),
            "what is left"
        );
        let ahead = Some(read_at + Duration::from_secs(3600));
        assert_eq!(
            racy_until(ahead, read_at, now),
            Some(now + RACY),
            "dated an hour ahead, read again for two seconds"
        );
    }

    /// A directory dated an hour ahead is read again for two seconds, as
    /// any other just changed, and its window is kept, not renewed, by the
    /// readings in it: not for the hour until the clocks meet.
    #[cfg(unix)]
    #[test]
    fn a_directory_dated_ahead_is_read_again_only_for_its_window() {
        let dir = tree("dated-ahead");
        let hour = SystemTime::now() + Duration::from_secs(3600);
        std::fs::File::open(&dir)
            .unwrap()
            .set_modified(hour)
            .unwrap();
        let mut ex = Explorer::open(&dir, false);
        let root = ex.root.clone();
        let until = ex.listed[&root].racy_until.expect("read again for now");
        assert!(until <= Instant::now() + RACY, "for the window at most");
        assert!(!ex.changed(), "nothing new");
        assert_eq!(
            ex.listed[&root].racy_until,
            Some(until),
            "kept, not renewed"
        );
    }

    #[cfg(unix)]
    #[test]
    fn names_that_are_not_utf8_still_resolve() {
        use std::os::unix::ffi::OsStrExt;
        let dir = tree("nonutf8");
        let raw = std::ffi::OsStr::from_bytes(b"caf\xe9.json");
        if let Err(e) = std::fs::write(dir.join(raw), "1") {
            // APFS on macOS, among others, refuses names that are not
            // valid UTF-8; there is nothing to test on such a filesystem.
            eprintln!("skipping: this filesystem refuses non-UTF-8 names ({e})");
            return;
        }
        let ex = Explorer::open(&dir, false);
        let doc = ex.build();
        let node = doc
            .children(0)
            .find(|&c| doc.node(c).key.name().is_some_and(|n| n.starts_with("caf")))
            .expect("the entry is listed under a readable label");
        let key = doc.node(node).key.name().unwrap().to_string();
        assert!(key.contains('\u{fffd}'), "{key}");
        let path = ex.fs_path(&doc.path(node));
        assert_eq!(path.file_name().unwrap().as_bytes(), b"caf\xe9.json");
        assert!(path.exists(), "the real file is what Enter would open");
    }

    #[test]
    fn rerooting_keeps_only_listings_under_the_new_root() {
        let dir = tree("adopt");
        let old = Explorer::open(&dir, false);
        let old_root = old.root.clone();
        let mut down = Explorer::open(&old_root.join("sub"), false);
        down.adopt_listings(old.clone());
        assert!(!down.is_listed(&old_root), "the old root is outside");
        assert!(down.is_listed(&old_root.join("sub")));
        let mut up = Explorer::open(old_root.parent().unwrap(), false);
        up.adopt_listings(old);
        assert!(up.is_listed(&old_root) && up.is_listed(&old_root.join("sub")));
    }

    #[test]
    fn unreadable_directory_shows_its_error() {
        let ex = Explorer::open(Path::new("/definitely/not/here"), false);
        let doc = ex.build();
        assert_eq!(doc.root().children, 1);
        assert_eq!(doc.node(1).key.name(), Some("(error)"));
    }

    #[test]
    fn verbatim_prefixes_are_stripped() {
        let f = |s: &str| simplify(PathBuf::from(s)).to_str().unwrap().to_string();
        assert_eq!(f(r"\\?\C:\Users\x"), r"C:\Users\x");
        assert_eq!(f(r"\\?\UNC\srv\share\x"), r"\\srv\share\x");
        assert_eq!(f("/usr/local"), "/usr/local");
        assert_eq!(f(r"C:\plain"), r"C:\plain");
    }

    #[test]
    fn sizes_and_titles() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(1023), "1023 B");
        assert_eq!(human_size(1536), "1.5 KB");
        assert_eq!(human_size(40 << 20), "40 MB");
        assert_eq!(human_size(3 << 30), "3.0 GB");
        assert_eq!(title_of(Path::new("/x/y/src")), "src/");
        assert_eq!(title_of(Path::new("/")), "/");
    }
}

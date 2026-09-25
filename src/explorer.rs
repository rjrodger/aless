//! The file explorer: a directory tree shown through the same machinery as
//! a document. Directories are containers, listed lazily as they are
//! expanded and one level ahead of that (so a collapsed directory's
//! preview can show its count and first names); files are leaves showing
//! size and format. `Enter` on a file opens it in a new tab.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use crate::doc::{Doc, Key, Kind, Node, NodeId, NO_NODE};
use crate::load::Format;

/// Directories listed at most, per explorer, so a deep expansion of a
/// huge tree stays bounded.
pub const MAX_LISTED: usize = 2000;

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
    /// The format the extension implies, when a grammar handles it.
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
    let mtime = std::fs::metadata(dir).ok().and_then(|m| m.modified().ok());
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
                        .then(|| {
                            Path::new(&name)
                                .extension()
                                .and_then(|x| x.to_str())
                                .and_then(Format::from_extension)
                        })
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
            }
        }
        Err(e) => Listing {
            entries: Vec::new(),
            mtime,
            error: Some(e.to_string()),
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

    /// Has any listed directory changed on disk since it was read?
    pub fn changed(&self) -> bool {
        self.listed.iter().any(|(dir, listing)| {
            std::fs::metadata(dir).ok().and_then(|m| m.modified().ok()) != listing.mtime
        })
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

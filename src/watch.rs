//! File watching: a `notify` watcher on each watched file's directory
//! (editors replace files by rename, which a watch on the file itself
//! would not survive), forwarding change notifications as [`Input`]s. The
//! application also polls file stamps on its tick, so a missed event only
//! delays a reload rather than losing it.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;

use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use crate::app::Input;

pub struct FileWatcher {
    watcher: RecommendedWatcher,
    /// Watched directories and how many files each serves.
    dirs: HashMap<PathBuf, usize>,
}

impl FileWatcher {
    /// Start a watcher whose notifications go to `tx`. `None` when the
    /// platform watcher cannot start (the poll fallback still works).
    pub fn new(tx: Sender<Input>) -> Option<FileWatcher> {
        let watcher = RecommendedWatcher::new(
            move |res: notify::Result<notify::Event>| {
                let Ok(event) = res else { return };
                if matches!(event.kind, EventKind::Access(_)) {
                    return;
                }
                for path in event.paths {
                    let _ = tx.send(Input::FileChanged(path));
                }
            },
            Config::default(),
        )
        .ok()?;
        Some(FileWatcher {
            watcher,
            dirs: HashMap::new(),
        })
    }

    fn dir_of(file: &Path) -> PathBuf {
        let parent = file.parent().filter(|p| !p.as_os_str().is_empty());
        let dir = parent
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        std::fs::canonicalize(&dir).unwrap_or(dir)
    }

    pub fn watch(&mut self, file: &Path) -> notify::Result<()> {
        let dir = Self::dir_of(file);
        let count = self.dirs.entry(dir.clone()).or_insert(0);
        if *count == 0 {
            self.watcher.watch(&dir, RecursiveMode::NonRecursive)?;
        }
        *count += 1;
        Ok(())
    }

    pub fn unwatch(&mut self, file: &Path) {
        let dir = Self::dir_of(file);
        if let Some(count) = self.dirs.get_mut(&dir) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                let _ = self.watcher.unwatch(&dir);
                self.dirs.remove(&dir);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn change_in_watched_directory_is_reported() {
        let dir = std::env::temp_dir().join(format!("aless-watch-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("w.json");
        std::fs::write(&file, "1").unwrap();
        let (tx, rx) = mpsc::channel();
        let Some(mut w) = FileWatcher::new(tx) else {
            eprintln!("no platform watcher here; skipping");
            return;
        };
        w.watch(&file).unwrap();
        w.watch(&file).unwrap();
        assert_eq!(w.dirs.len(), 1);
        std::thread::sleep(Duration::from_millis(200));
        std::fs::write(&file, "2").unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut seen = false;
        while std::time::Instant::now() < deadline {
            match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(Input::FileChanged(p)) if p.file_name() == file.file_name() => {
                    seen = true;
                    break;
                }
                Ok(_) => {}
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => break,
            }
        }
        assert!(seen, "no change notification arrived");
        w.unwatch(&file);
        assert_eq!(w.dirs.len(), 1, "still one watcher on the directory");
        w.unwatch(&file);
        assert!(w.dirs.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

---
title: How a reload keeps your place
description: What aless does when a watched file changes, why it finds your place again by path, and what happens when the file breaks or disappears.
order: 3
---
A viewer that reloads a changed file has to decide where to put you afterwards. Putting you back at the top is simple and makes watching useless, since you lose your place on every save. Keeping the same screen row is simple too, and wrong as soon as an edit adds or removes a line above you. aless keeps the node you were reading.

## Finding the node again

A reload builds a new tree from the new text, then re-anchors the view on it in three steps:

1. Folds are remembered by path. A container that is still there keeps its folded or unfolded state, and a new container starts unfolded.
2. The focused node is found again by its path. When that path is gone, the focus goes to its nearest ancestor that survived, and within that ancestor to the node closest to the source line the old focus was on, so a renamed key or a reordered entry keeps you near where you were reading.
3. The focused row stays on the same screen row, so the screen does not jump.

A path is the right identity for a node across edits because it is what stays the same when lines move: inserting a block above a value changes its line and leaves its path alone. It is the wrong identity for a renamed key, which is why the line comes back as the second test.

## Seeing every change

aless asks the operating system's file watcher for changes (inotify on Linux, FSEvents on macOS, ReadDirectoryChangesW on Windows), and watches the file's directory rather than the file, since many editors save by writing a temporary file and renaming it over the old one, which a watch on the old file would miss. Changes are gathered for a moment before the reload, so a save made of several writes reloads once. As well as the watcher, aless checks each watched file's size and modification time every half second, which catches whatever the watcher misses.

## When the file breaks or goes

A save that does not parse keeps the previous document on screen, with the parser's report docked beneath it, because the moment you are editing a file is the moment you most want to see what it held. The next save that parses replaces both. A file that disappears keeps its document and is marked as gone, and reloads when it comes back, as files rewritten by build tools often do. A file that does not exist yet opens in a tab that waits for it.

None of this is free. Each reload parses the whole file again, and the viewer pauses while it does, so a large file that changes every second keeps the viewer busy. `--no-watch` and `W` are there for that case: [Follow a file as it changes](/how-to/watch.html).

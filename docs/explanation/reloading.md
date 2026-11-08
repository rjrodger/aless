---
title: How a reload keeps your place
description: Why aless finds your place again by path after a watched file changes, how aless sees every change, and what happens when the file breaks or disappears.
order: 3
---
After a reload, aless puts you back on the node you were reading. A viewer that reloads a changed file must decide where to put you afterwards. The two other answers are less work to build, but both lose your place. Sent back to the top, you lose your place on every save, which makes watching useless. Kept on the same screen row, you sit on the wrong node as soon as an edit adds or removes a line above you.

## Finding the node again

A reload builds a new tree from the new text. aless then sets the view on the new tree in three steps:

1. Folds are restored by path. A container that is still there keeps its folded or unfolded state. A new container starts unfolded.
2. The focused node is found again by its path. When that path is gone, the focus goes to the node's nearest ancestor that survived. Within that ancestor, the focus goes to the node closest to the line the old focus was on.
3. The focused row stays on the same screen row, so the screen does not jump. The one exception is the reload that brings a file's second version: the timeline's row comes from the pane, and a focus at the foot of the pane moves up a row to make room.

Across an edit, a path is the right identity for a node, because the path stays the same when lines move. A block inserted above a value changes the value's line and leaves the value's path alone. For a renamed key, though, the path is the wrong identity. That is why the line is the second test.

## Seeing every change

aless asks the operating system's file watcher for changes. That watcher is inotify on Linux, FSEvents on macOS, and ReadDirectoryChangesW on Windows. aless watches the file's directory rather than the file, because many editors save by writing a temporary file and renaming it over the old one. A watch on the old file would miss that save.

aless gathers changes for a moment before it reloads, so a save made of several writes reloads once.

While the viewer is idle, aless also checks each watched file's size and modification time about every half second. That check catches what the watcher misses.

## Keeping the document when the file breaks or disappears

A save that does not parse keeps the previous document on screen, because the moment you are editing a file is the moment you most want to see what the file held. aless docks the parser's report beneath that document. The next save that parses replaces both.

A file that disappears also keeps its document on screen. aless marks the file as gone. When the file comes back, aless reloads it. When a build tool rewrites a file, the file often disappears and then comes back. A file that does not exist yet opens in a tab that waits for it.

## Marking what a save changed

Each save that parses is a version. aless compares the new version's tree with the version before it, path by path. A value whose path is new is added, a value whose path is gone is removed, and a value whose text or kind differs at the same path is changed. When a container becomes another kind, such as an object that becomes an array, aless counts one change and does not look inside. An array is compared index by index, so an item inserted at its start changes every index after it.

Each version keeps the changes from the version before it, so a version shows the same marks whenever it comes back on screen. Two quick saves show only the second save's changes until you go back a version.

A version keeps its text, not its tree. A tree takes many times the memory of its text, so aless parses a version's text again when the version comes back on screen. aless keeps up to 50 versions of a file, and lets the oldest go sooner when their texts together pass the size aless reads by default. The version on screen and the newest always stay.

A save that does not parse is not a version, because it has no tree to show. It shows on the timeline as a `!` at the time of the save.

## Paying for each reload

Reloading is not free. Each reload parses the whole file again, and the viewer waits for the parse. A large file that changes every second therefore keeps the viewer busy. `--no-watch` and `W` are there for that case. For more information, see [Follow a file as it changes](/how-to/watch.html).

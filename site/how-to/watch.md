---
title: Follow a file as it changes
description: Keep a file open in the viewer while it is written, rewritten or regenerated, with your place kept, and stop following it when you want it still.
order: 4
---
Every file aless opens in the viewer is watched, and reloaded when it changes on disk. There is nothing to turn on: the status bar says `watching`.

## Keep a file open while something writes it

Open it as usual:

```sh
aless build/report.json
```

Then let your build, your tests or your editor write it. aless reloads it on each change it sees, through the operating system's file watcher, and a check every half second catches whatever the watcher misses. It watches the file's directory, so an editor that saves by writing a new file and renaming it over the old one is seen too, and a save made of several writes reloads once.

After a reload your view is where you left it. The folds you made stay made, and the focus stays on the node you were reading, found again by its path. When that node is gone, the focus moves to its nearest surviving ancestor, at the node closest to the line it was on. [How a reload keeps your place](/explanation/reloading.html) says why it works that way.

## When a save breaks the file

A save that does not parse does not take your document away. aless keeps the last version that parsed on screen, docks the parser's report beneath it, and marks the tab with `!`. The next save that parses clears it. Press `!` to see the report full size.

A file that is deleted keeps its document on screen, and its tab is marked `✗`; when the file comes back, aless reloads it. A file that does not exist yet can be opened all the same, and its tab waits for it.

## Stop following a file

`W`, or `:watch off`, stops watching the focused tab, and `W` again starts. `r`, or `:reload`, reloads it now, watched or not. To open files without watching any of them:

```sh
aless --no-watch big.json
```

A large file pauses the viewer while it parses, on every reload, so `--no-watch` suits a big file that changes often.

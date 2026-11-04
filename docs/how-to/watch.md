---
title: Follow a file as it changes
description: Keep a file open in the viewer while it is written, rewritten or regenerated, with your place kept, and stop following it when you want it still.
group: Files and formats
order: 8
---
aless watches every file it opens, and reads it again when it changes on disk. There is nothing to turn on: the status bar says `watching`.

## Keep a file open while something writes it

Open it as usual:

```sh
aless build/report.json
```

Then let your build, your tests or your editor write it. aless reads the file again on each change it sees. It hears of a change from the operating system's file watcher, and while the viewer is idle it also checks the file's size and modification time about every half second, which catches what the watcher misses. It watches the file's directory, so an editor that saves by writing a new file and renaming it over the old one is seen too, and a save made of several writes reloads once.

After a reload your view is where you left it. The folds you made stay made, and the focus stays on the node you were reading, found again by its path. When that node is gone, the focus moves to its nearest surviving ancestor, at the node closest to the line it was on. [How a reload keeps your place](/explanation/reloading.html) says why it works that way.

## When a save breaks the file

A save that does not parse does not take your document away. aless keeps the last version that parsed on screen, and docks the parser's report beneath it. Here a comma too many went into the first book in [`books.json`](/examples/books.json):

<!-- screen: aless books.json
edit: books.json: "year": 1999, => "year": 1999,,
size: 80x24 -->
```screen
▼ [
  ▽ {
      title: "A Book of Examples"
      author: "A. N. Author"
      year: 1999
    ▽ tags: [
        "examples"
        "reference"
  ▽ {
      title: "Shelves and How to Fill Them"
      author: "B. Bookworm"
── parse error · ! shows the full report ───────────────────────────────────────
[tabnas/unexpected]: unexpected character(s): ,
  --> books.json:5:18
  3 |     "title": "A Book of Examples",
  4 |     "author": "A. N. Author",
  5 |     "year": 1999,,
                       ^ unexpected character(s): ,
  6 |     "tags": ["examples", "reference"]
  7 |   },

  …
 books.json .          !5:18: unexpected character(s): ,  json · watching · 1:1
books.json: 5:18: unexpected character(s): , — ! shows the report
```

The status bar carries the short form, a `!`, the line and column, and the message, and with several tabs open, the tab's name in the strip is marked `!`. Press `!` to see the report full size, where `h` and `l` pan a line wider than the screen. The next save that parses clears it. [Find out why a file does not parse](/how-to/errors.html) says how to read the report.

A file that is deleted keeps its document on screen, the status bar says `file gone`, and with several tabs open its name is marked `✗`. When the file comes back, aless reads it again. A file that does not exist yet can be opened all the same, and its tab waits for it.

## Stop following a file

`W`, or `:watch off`, stops watching the focused tab, and `W` again, or `:watch on`, starts. `r`, or `:reload`, reads the file again now, watched or not. To open files without watching any of them:

```sh
aless --no-watch big.json
```

A large file holds the viewer while it parses, on every reload, so `--no-watch` suits a big file that changes often. [Large inputs and limits](/explanation/large-inputs.html) says what a parse costs.

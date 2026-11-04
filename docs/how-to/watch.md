---
title: Follow a file as it changes
description: Keep a file open in the viewer while the file is written, rewritten, or regenerated, with your place kept on each reload, and stop following the file when you want the document on screen left as it is.
group: Files and formats
order: 8
---
aless watches every file it opens. When a file changes on disk, aless reads that file again. There is nothing to turn on: the status bar says `watching`.

## Keep a file open while something writes it

Open the file as usual:

<!-- test: skip build/report.json stands for a file the reader's build writes, which the tests do not have -->
```sh
aless build/report.json
```

Then let your build, your tests, or your editor write the file. aless reads the file again on each change it sees.

The operating system's file watcher reports changes to aless. While the viewer is idle, aless also checks the file's size and modification time about every half second. That check catches what the watcher misses. aless watches the file's directory, so it sees a save in which the editor writes a new file and renames that file over the old one. A save made of several writes reloads once.

A reload keeps your place. The folds you made stay made. The focus stays on the node you were reading, because aless finds that node again by its path. When that node is gone, the focus moves to the node's nearest surviving ancestor. Within that ancestor, the focus goes to the node closest to the line the old focus was on. For more information, see [How a reload keeps your place](/explanation/reloading.html).

## Keep reading when the file breaks or disappears

A save that doesn't parse doesn't take your document away. The last version that parsed stays on screen. Beneath that version, aless docks the parser's report. In this example, a comma too many has gone into the first book in [`books.json`](/examples/books.json):

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
books.json: 5:18: unexpected character(s): , (! shows the report)
```

The status bar carries the short form: a `!`, the line and column, and the message. With several tabs open, aless also marks the tab's name in the strip with `!`. To see the report full size, press `!`. In the full-size report, `h` and `l` pan a line wider than the screen. The next save that parses clears the report. For more information, see [Find out why a file does not parse](/how-to/errors.html).

A file that is deleted keeps its document on screen, so you can go on reading what the file held. The status bar says `file gone`. With several tabs open, aless also marks the tab's name with `✗`. When the file comes back, aless reads the file again.

You can open a file that doesn't exist yet, such as `build/report.json` before your build first writes it. Its tab waits for the file.

## Stop following a file

To stop watching the active tab, press `W` or type `:watch off`. To start watching the tab again, press `W` a second time or type `:watch on`. To read the file again now, whether the tab is watched or not, press `r` or type `:reload`. To open files without watching any of them, pass `--no-watch`:

```sh
aless --no-watch books.json
```

<!-- screen: aless --no-watch books.json
size: 80x4 -->
```screen
▼ [
  ▽ {
 books.json .                                                        json · 1:1
```

The status bar no longer says `watching`.

Every reload parses the file again. A large file holds the viewer while aless parses it, so `--no-watch` suits a large file that changes often. For more information, see [Large inputs and limits](/explanation/large-inputs.html).

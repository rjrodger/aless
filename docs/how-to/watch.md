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

## See what a save changed

Each save that parses is a new version of the file. aless marks what the new version changed from the one before. A changed value turns yellow, and a new value turns green. A container that lost values counts them in red, as `−1`. Rails in the indentation lead from the root down to each change, so you can find a change from anywhere above it. In this example, the year of the first book in [`books.json`](/examples/books.json) changed from 1999 to 2001:

<!-- screen: aless books.json
edit: books.json: "year": 1999, => "year": 2001,
size: 80x24 -->
```screen
▼ [
└─▽ {
  │   title: "A Book of Examples"
  │   author: "A. N. Author"
  └─~ year: 2001
    ▽ tags: [
        "examples"
        "reference"
  ▽ {
      title: "Shelves and How to Fill Them"
      author: "B. Bookworm"
      year: 2012
    ▽ tags: [
        "furniture"
  ▽ {
      title: "The Index"
      author: "C. Cataloguer"
      year: 2021
      tags: []
── 1 change in version 2 · [ older · d next change ─────────────────────────────
  ~ [0].year  1999 → 2001
 14:00:05 ○──────────────────────────────────────────────────● 14:02:11  2 of 2
 books.json .                                             json · watching · 1:1
Reloaded books.json: 1 changed
```

The note under the tree lists each change, with the value before and after. The next key closes the note, and the marks stay until the next version. To go to each change in turn, press `d`. To go back a change, press `D`.

A folded container counts the changes inside it, such as `~1 +1`. In the source view, a column before the line numbers marks each line that holds a change.

## Go back to an earlier version

aless keeps up to 50 versions of each file it watches, and fewer of a large file. Above the status bar, a timeline shows a dot for each version, spaced by the time between the saves. The dot of the version on screen is lit. A save that doesn't parse shows as a red `!`.

To show the version before the one on screen, press `[`. To show the version after it, press `]`. `{` shows the oldest version aless keeps, and `}` shows the newest. A count moves further: `3[` goes back three versions. Here `[` has gone back to the file as it was opened:

<!-- screen: aless books.json
edit: books.json: "year": 1999, => "year": 2001,
then: [
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
      year: 2012
    ▽ tags: [
        "furniture"
  ▽ {
      title: "The Index"
      author: "C. Cataloguer"
      year: 2021
      tags: []

── Version 1 of 2, opened 14:00:05 · ] newer · } newest · d next change ────────
 14:00:05 ●──────────────────────────────────────────────────○ 14:02:11  1 of 2
 books.json .                            json · watching · version 1 of 2 · 1:1
```

While an older version is on screen, the status bar turns yellow and names the version. A save that arrives in the meantime adds a version to the timeline and leaves the screen as it is. The message line says so, and `}` shows the new version. Your place carries across versions as it does across a reload.

To keep more or fewer versions, type `:set history=N`. To stop marking changes, type `:set nochanges`, or start aless with `--no-changes`. `:format` starts the versions again, because trees read in different formats do not compare.

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
 14:00:05 ●──────────────────────────────────────────────────! 14:02:11  1 of 1
 books.json .          !5:18: unexpected character(s): ,  json · watching · 1:1
books.json: 5:18: unexpected character(s): , (! shows the report)
```

The status bar carries the short form: a `!`, the line and column, and the message. The timeline above the status bar marks the failed save with a `!` of its own. With several tabs open, aless also marks the tab's name in the strip with `!`. To see the report full size, press `!`. In the full-size report, `h` and `l` pan a line wider than the screen. The next save that parses clears the report. For more information, see [Find out why a file does not parse](/how-to/errors.html).

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

---
title: Compare the versions of a file
description: See what each save of a watched file changed, step back through the versions aless keeps, list them in the history tab, and keep one version as a file.
group: Files and formats
order: 9
---
Each save of a watched file that parses is a new version of the file. aless keeps up to 50 versions of each file, and marks what each version changed from the one before. The examples on this page edit [`books.json`](/examples/books.json) while aless shows it. To follow a file in the first place, see [Follow a file as it changes](/how-to/watch.html).

## Read the marks of a save

A save can change values, add values, and remove values, and each kind of change has a mark of its own. In this example, one save changed the first book's year, removed its tag `"reference"`, and gave the second book the tag `"shelving"`:

<!-- screen: aless books.json
edit: books.json: "year": 1999,\n    "tags": ["examples", "reference"]\n  },\n  {\n    "title": "Shelves and How to Fill Them",\n    "author": "B. Bookworm",\n    "year": 2012,\n    "tags": ["furniture"] => "year": 2001,\n    "tags": ["examples"]\n  },\n  {\n    "title": "Shelves and How to Fill Them",\n    "author": "B. Bookworm",\n    "year": 2012,\n    "tags": ["furniture", "shelving"]
size: 80x26 -->
```screen
▼ [
├─▽ {
│ │   title: "A Book of Examples"
│ │   author: "A. N. Author"
│ ├─~ year: 2001
│ └─▽ tags: [ −1
│       "examples"
└─▽ {
  │   title: "Shelves and How to Fill Them"
  │   author: "B. Bookworm"
  │   year: 2012
  └─▽ tags: [
    │   "furniture"
    └─+ "shelving"
  ▽ {
      title: "The Index"
      author: "C. Cataloguer"
      year: 2021
      tags: []
── 3 changes in version 2 · [ older · d next change ────────────────────────────
  ~ [0].year     1999 → 2001
  − [0].tags[1]  "reference"
  + [1].tags[1]  "shelving"
 14:00:05 ○──────────────────────────────────────────────────● 14:02:11  2 of 2
 books.json .                                             json · watching · 1:1
Reloaded books.json: 1 changed, 1 added, 1 removed
```

Each mark pairs a sign with a colour, so the sign alone tells the change apart:

- A changed value has `~` before it, and the value turns yellow.
- A new value has `+` before it, and the value turns green.
- A container that lost values counts them after its bracket, as `−1`, in red.
- Rails in the indentation lead from the root down to each change.

The note under the tree lists each change, with the value before and after. The next key closes the note, and the marks stay until the next version. To draw the marks and the rails with ASCII characters only, type `:set ascii`.

## Find a change inside a folded container

A folded container counts the changes inside it, kind by kind. Here a save changed the first book's year and removed one of its tags. The first book is then folded, so its row counts one changed value and one removed value:

<!-- screen: aless books.json
edit: books.json: "year": 1999,\n    "tags": ["examples", "reference"] => "year": 2001,\n    "tags": ["examples"]
then: j h
size: 80x8 -->
```screen
▽ [
└─▶ (4) {title: "A Book of Examples", author: "A. N. Author", …}  ~1 −1
  ▽ {
      title: "Shelves and How to Fill Them"
      author: "B. Bookworm"
 14:00:05 ○──────────────────────────────────────────────────● 14:02:11  2 of 2
 books.json [0]                                           json · watching · 2:3
```

The rails still lead to the folded row, so you can find a change from anywhere above it. To unfold the row, press `l`.

## Go from change to change

To go to the next change, press `d`. To go to the change before, press `D`. Both keys go round the end of the document, and both leave the note open. Here `d` has gone to the first change, and the status bar shows its path:

<!-- screen: aless books.json
edit: books.json: "year": 1999,\n    "tags": ["examples", "reference"] => "year": 2001,\n    "tags": ["examples"]
then: d
size: 80x16 -->
```screen
▽ [
└─▽ {
  │   title: "A Book of Examples"
  │   author: "A. N. Author"
  ├─~ year: 2001
  └─▽ tags: [ −1
        "examples"
  ▽ {
      title: "Shelves and How to Fill Them"
      author: "B. Bookworm"
── 2 changes in version 2 · [ older · d next change ────────────────────────────
  ~ [0].year     1999 → 2001
  − [0].tags[1]  "reference"
 14:00:05 ○──────────────────────────────────────────────────● 14:02:11  2 of 2
 books.json [0].year                                      json · watching · 5:5
Change 1 of 2: [0].year changed
```

## Read the timeline

Above the status bar, a timeline shows each version as a dot. The dots are spaced by the time between the saves, so a burst of saves shows as a cluster. The dot of the version on screen is filled. A save that doesn't parse shows as a `!` at the time of the save. In this example, the file was saved four times, and the second save had a comma too many:

<!-- screen: aless books.json
edit: books.json: "year": 1999, => "year": 2001,
edit: books.json: "year": 2001, => "year": 2001,,
edit: books.json: "year": 2001,, => "year": 2002,
edit: books.json: "tags": [] => "tags": ["indexes"]
size: 80x24
rows: 22-24 -->
```screen
 14:00:05 ○─────────○────────────────!────○──────────────────● 14:10:55  4 of 4
 books.json .                                             json · watching · 1:1
Reloaded books.json: 1 added
```

The times at the ends of the timeline are the first and the last saves aless keeps. `4 of 4` says that the newest of four versions is on screen. The save that didn't parse is not a version, because it has no tree to show.

## Step back through the versions

To show the version before the one on screen, press `[`. To show the version after it, press `]`. `{` shows the oldest version aless keeps, and `}` shows the newest. A count moves further: `2[` goes back two versions. Here `[` has gone back from version 3 to version 2:

<!-- screen: aless books.json
edit: books.json: "year": 1999, => "year": 2001,
edit: books.json: "tags": [] => "tags": ["indexes"]
then: [
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
── 1 change in version 2 · ] newer · } newest · d next change ──────────────────
  ~ [0].year  1999 → 2001
 14:00:05 ○──────────────────●───────────────────────────────○ 14:05:47  2 of 3
 books.json .                            json · watching · version 2 of 3 · 1:1
```

While an older version is on screen, the status bar turns yellow and names the version. The note names the version too, and shows its own changes. Your place carries across versions, as it does across a reload.

A save that arrives while you read an older version adds a version to the timeline and leaves the screen as it is. The message line says which version is the newest:

<!-- screen: aless books.json
edit: books.json: "year": 1999, => "year": 2001,
then: [
edit: books.json: "tags": [] => "tags": ["indexes"]
size: 80x24
rows: 21-24 -->
```screen
── Version 1 of 3, opened 14:00:05 · ] newer · } newest · d next change ────────
 14:00:05 ●──────────────────○───────────────────────────────○ 14:05:47  1 of 3
 books.json .                            json · watching · version 1 of 3 · 1:1
books.json changed at 14:05:47: version 3 is the newest. } shows it
```

To show the newest version, press `}`.

## See the changes in the source view

The source view marks each line that holds a change, in a column before the line numbers. To open the source view, press `s`:

<!-- screen: aless books.json
edit: books.json: "year": 1999,\n    "tags": ["examples", "reference"] => "year": 2001,\n    "tags": ["examples"]
then: s
size: 80x14 -->
```screen
   1▶ [
   2    {
   3      "title": "A Book of Examples",
   4      "author": "A. N. Author",
~  5      "year": 2001,
−  6      "tags": ["examples"]
   7    },
   8    {
   9      "title": "Shelves and How to Fill Them",
  10      "author": "B. Bookworm",
  11      "year": 2012,
 14:00:05 ○──────────────────────────────────────────────────● 14:02:11  2 of 2
 books.json  (source)                                     json · watching · 1:1
```

To go back to the tree, press `s` again.

## List the versions in the history tab

To list every version aless keeps, press `v` or type `:history`. The history tab opens with the newest version first. Each version holds its changes, and the version on screen in the file's tab is unfolded. A save that didn't parse has a row of its own, at the time of the save:

<!-- screen: aless books.json
edit: books.json: "year": 1999, => "year": 2001,
edit: books.json: "year": 2001, => "year": 2001,,
edit: books.json: "year": 2001,, => "year": 2002,
then: v
size: 80x10 -->
```screen
 1:books.json*   2:books.json history
▽ books.json  3 versions, and a save that did not parse
  ▼ version 3  14:06:49  ~1  the newest, on screen
    ~ [0].year  2001 → 2002
  ! did not parse  14:05:47  5:18: unexpected character(s): ,
  ▷ version 2  14:02:11  ~1
    version 1  14:00:05

 books.json history version 3                              history · 3 versions
books.json: 3 versions. Enter opens one in a tab of its own
```

The history tab follows the file, so each save adds a row. `/` searches the text of the rows, such as a path or a value. The history tab closes when you close the file's tab.

## Open a version in a tab of its own

To open a version in a tab of its own, focus the version in the history tab and press `Enter`. Focus a change instead, and the version opens at that change. The version's tab isn't watched, so it keeps that version while the file goes on changing. Its tab name ends in the version's number:

<!-- screen: aless books.json
edit: books.json: "year": 1999, => "year": 2001,
then: v j Enter
size: 80x16 -->
```screen
 1:books.json*   2:books.json history   3:books.json v2
▽ [
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
── 1 change in version 2 · d next change · v history ───────────────────────────
  ~ [0].year  1999 → 2001
 books.json v2 [0].year                                  json · version 2 · 5:5
Version 2 of books.json, saved 14:02:11, in a tab of its own
```

To go back to the history tab from a version's tab, press `v`. A version that is already open in a tab opens in that tab again.

## Keep a version as a file

To write a version to a file, type `:ws FILE` for its text as it was saved, or `:w FILE` for its tree as JSON. In the history tab, both commands write the focused version. Elsewhere, they write the version on screen, so `[` and then `:ws FILE` keeps the text of the version before. Neither command writes over a file that exists, unless you type `:ws!` or `:w!`.

To print the tree of a file as JSON from the shell, without the viewer, see [Save a document as JSON](/how-to/json.html#print-the-json-from-the-shell).

## Keep more versions, or none of the marks

To keep more or fewer versions, type `:set history=N`, where `N` is the number of versions. aless also lets the oldest versions go sooner when their texts together pass 64 MB. The version on screen and the newest always stay.

To stop marking changes, type `:set nochanges`, or start aless with `--no-changes`. aless still keeps the versions, so the timeline and the history tab still work:

<!-- screen: aless --no-changes books.json
edit: books.json: "year": 1999, => "year": 2001,
size: 80x10 -->
```screen
▼ [
  ▽ {
      title: "A Book of Examples"
      author: "A. N. Author"
      year: 2001
    ▽ tags: [
        "examples"
 14:00:05 ○──────────────────────────────────────────────────● 14:02:11  2 of 2
 books.json .                                             json · watching · 1:1
Reloaded books.json: 1 changed
```

To mark changes again, type `:set changes`. For more information about how aless compares two versions, see [How a reload keeps your place](/explanation/reloading.html).

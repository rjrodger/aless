---
title: Watch the changes to a file
description: Keep a JSON file open in aless while you change it in an editor, and read the marks aless puts on each change, from a changed value to a removed one, and the rails that lead to them.
order: 2
---
In this tutorial we keep a small JSON file open in aless, change the file in an editor, and read what aless marks after each save. The lesson takes about 10 minutes.

You need aless 0.8.0 or later. For more information, see [Install aless](/how-to/install.html). You also need an editor, and a terminal at least 80 columns wide.

## Get the file

The file holds three made-up book records. Download it into an empty directory:

<!-- test: skip it downloads the file the site publishes, which the screens on this page read from examples/ -->
```sh
curl -LO https://aless.tabnas.dev/examples/books.json
```

Or open [`books.json`](/examples/books.json) in your browser and save it into the same directory.

## Open the file

To open the file, give aless the file's name:

```sh
aless books.json
```

aless draws the file as a tree:

<!-- screen: aless books.json
size: 80x22 -->
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

 books.json .                                             json · watching · 1:1
```

The status bar says `watching`. aless watches every file it opens, so it reads the file again each time the file changes.

## Change a value

Leave aless open. Make the first change in your editor:

1. Open `books.json`.
2. Change the first book's year from `1999` to `2001`.
3. Save the file.

Within a second, aless reads the file again and marks the change:

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

aless marks the change in three ways:

- The year has a `~` before it, which marks a changed value, and the value turns yellow.
- Rails in the indentation lead from the root down to the change. They pass the first book, so the change can be found from the top of the file.
- The note under the tree lists the change, with the value before and after it: `1999 → 2001`.

Above the status bar, a timeline has appeared. It has a dot for each version of the file: one for the file as you opened it, and one for the save.

## Close the note

The note stays open until you press a key. Press `j` to move down a row:

<!-- screen: aless books.json
edit: books.json: "year": 1999, => "year": 2001,
then: j
size: 80x22 -->
```screen
▽ [
└─▼ {
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
 14:00:05 ○──────────────────────────────────────────────────● 14:02:11  2 of 2
 books.json [0]                                           json · watching · 2:3
```

The note has closed, but the marks are still there. They stay until the next save.

## Add a value

Now give the second book a second tag. In your editor, change `["furniture"]` to `["furniture", "shelving"]`, and save the file:

<!-- screen: aless books.json
edit: books.json: "year": 1999, => "year": 2001,
then: j
edit: books.json: ["furniture"] => ["furniture", "shelving"]
size: 80x24 -->
```screen
▽ [
│ ▼ {
│     title: "A Book of Examples"
│     author: "A. N. Author"
│     year: 2001
│   ▽ tags: [
│       "examples"
│       "reference"
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
── 1 change in version 3 · [ older · d next change ─────────────────────────────
  + [1].tags[1]  "shelving"
 14:00:05 ○──────────────────○───────────────────────────────● 14:05:47  3 of 3
 books.json [0]                                           json · watching · 2:3
Reloaded books.json: 1 added
```

The new tag has a `+` before it, which marks a new value, and the tag turns green. The year has lost its mark. Each save marks what it changed from the save before, so the marks always show the latest save.

## Remove a value

Now take the first book's second tag away. Change `["examples", "reference"]` to `["examples"]`, and save the file:

<!-- screen: aless books.json
edit: books.json: "year": 1999, => "year": 2001,
then: j
edit: books.json: ["furniture"] => ["furniture", "shelving"]
edit: books.json: ["examples", "reference"] => ["examples"]
size: 80x24 -->
```screen
▽ [
└─▼ {
  │   title: "A Book of Examples"
  │   author: "A. N. Author"
  │   year: 2001
  └─▽ tags: [ −1
        "examples"
  ▽ {
      title: "Shelves and How to Fill Them"
      author: "B. Bookworm"
      year: 2012
    ▽ tags: [
        "furniture"
        "shelving"
  ▽ {
      title: "The Index"
      author: "C. Cataloguer"
      year: 2021
      tags: []
── 1 change in version 4 · [ older · d next change ─────────────────────────────
  − [0].tags[1]  "reference"
 14:00:05 ○───────────────○──────────────────────────○───────● 14:06:49  4 of 4
 books.json [0]                                           json · watching · 2:3
Reloaded books.json: 1 removed
```

A removed value has no row left to mark, so its container counts it. `tags: [ −1` says that the array lost one value, and the count is red. The note names the value that went: `"reference"`.

## Fold the changed book

A folded container still shows the changes inside it. The focus is still on the first book, where `j` left it. Press `h` to fold the first book:

<!-- screen: aless books.json
edit: books.json: "year": 1999, => "year": 2001,
then: j
edit: books.json: ["furniture"] => ["furniture", "shelving"]
edit: books.json: ["examples", "reference"] => ["examples"]
then: h
size: 80x12 -->
```screen
▽ [
└─▶ (4) {title: "A Book of Examples", author: "A. N. Author", year: 2001, …}  −1
  ▽ {
      title: "Shelves and How to Fill Them"
      author: "B. Bookworm"
      year: 2012
    ▽ tags: [
        "furniture"
        "shelving"
 14:00:05 ○───────────────○──────────────────────────○───────● 14:06:49  4 of 4
 books.json [0]                                           json · watching · 2:3
```

The folded book ends in `−1`, the count of the change inside it. The rails still lead to the folded row, so a change stays easy to find in a long file.

## Jump to the change

To go to the next change, press `d`:

<!-- screen: aless books.json
edit: books.json: "year": 1999, => "year": 2001,
then: j
edit: books.json: ["furniture"] => ["furniture", "shelving"]
edit: books.json: ["examples", "reference"] => ["examples"]
then: h d
size: 80x12 -->
```screen
▽ [
└─▽ {
  │   title: "A Book of Examples"
  │   author: "A. N. Author"
  │   year: 2001
  └─▼ tags: [ −1
        "examples"
  ▽ {
      title: "Shelves and How to Fill Them"
 14:00:05 ○───────────────○──────────────────────────○───────● 14:06:49  4 of 4
 books.json [0].tags                                      json · watching · 6:5
Change 1 of 1: [0].tags: 1 removed
```

aless unfolds the first book to reach the change. The focus is on the array that lost its tag, and the status bar shows the array's path, `[0].tags`. `D` goes to the change before. Both keys go round the end of the file, so you can visit every change with one key.

## See the change in the text

To see the file's text, press `s`:

<!-- screen: aless books.json
edit: books.json: "year": 1999, => "year": 2001,
then: j
edit: books.json: ["furniture"] => ["furniture", "shelving"]
edit: books.json: ["examples", "reference"] => ["examples"]
then: h d s
size: 80x12 -->
```screen
   2    {
   3      "title": "A Book of Examples",
   4      "author": "A. N. Author",
   5      "year": 2001,
−  6▶     "tags": ["examples"]
   7    },
   8    {
   9      "title": "Shelves and How to Fill Them",
  10      "author": "B. Bookworm",
 14:00:05 ○───────────────○──────────────────────────○───────● 14:06:49  4 of 4
 books.json  (source)                                     json · watching · 6:5
```

A column before the line numbers marks each line that holds a change. Here `−` marks the line whose array lost a value. To go back to the tree, press `s` again.

That's the whole set of marks: `~` for a changed value, `+` for a new one, a count for removed ones, and rails that lead to each of them.

## Go further

aless kept every save you made in this lesson as a version of the file. The next lesson, [Look back through the versions of a file](/tutorials/history.html), steps back through those versions. For more information about each mark, see [Compare the versions of a file](/how-to/versions.html). For more information about how aless decides what changed, see [How a reload keeps your place](/explanation/reloading.html).

---
title: Look back through the versions of a file
description: Save a JSON file several times while aless shows it, then step back through the versions aless keeps, list them in the history tab, open an old version in a tab of its own, and keep it as a file.
order: 3
---
In this tutorial we save a small JSON file several times while aless shows it, and then look back through the versions aless keeps. The lesson takes about 10 minutes. The lesson before this one, [Watch the changes to a file](/tutorials/changes.html), shows how aless marks a change. This lesson starts again from a fresh copy of the same file.

You need aless 0.8.0 or later. For more information, see [Install aless](/how-to/install.html). You also need an editor, and a terminal at least 80 columns wide.

## Get the file

The file holds three made-up book records. Download it into an empty directory:

<!-- test: skip it downloads the file the site publishes, which the screens on this page read from examples/ -->
```sh
curl -LO https://aless.tabnas.dev/examples/books.json
```

Or open [`books.json`](/examples/books.json) in your browser and save it into the same directory.

## Save the file three times

To open the file, give aless the file's name:

```sh
aless books.json
```

Leave aless open, and make three changes in your editor, saving the file after each one:

1. Change the first book's year from `1999` to `2001`, and save.
2. Change `["furniture"]` to `["furniture", "shelving"]`, and save.
3. Change `"The Index"` to `"The Index, Revised"`, and save.

aless keeps each save as a version of the file:

<!-- screen: aless books.json
edit: books.json: "year": 1999, => "year": 2001,
edit: books.json: ["furniture"] => ["furniture", "shelving"]
edit: books.json: "The Index" => "The Index, Revised"
size: 80x24 -->
```screen
▼ [
│ ▽ {
│     title: "A Book of Examples"
│     author: "A. N. Author"
│     year: 2001
│   ▽ tags: [
│       "examples"
│       "reference"
│ ▽ {
│     title: "Shelves and How to Fill Them"
│     author: "B. Bookworm"
│     year: 2012
│   ▽ tags: [
│       "furniture"
│       "shelving"
└─▽ {
  └─~ title: "The Index, Revised"
      author: "C. Cataloguer"
      year: 2021
── 1 change in version 4 · [ older · d next change ─────────────────────────────
  ~ [2].title  "The Index" → "The Index, Revised"
 14:00:05 ○───────────────○──────────────────────────○───────● 14:06:49  4 of 4
 books.json .                                             json · watching · 1:1
Reloaded books.json: 1 changed
```

The timeline above the status bar has a dot for each version. The first dot is the file as you opened it, and the other three are your saves. The dots are spaced by the time between the saves. The dot of the version on screen is filled, and `4 of 4` says that the newest of the four versions is on screen.

## Step back a version

To show the version before the one on screen, press `[`:

<!-- screen: aless books.json
edit: books.json: "year": 1999, => "year": 2001,
edit: books.json: ["furniture"] => ["furniture", "shelving"]
edit: books.json: "The Index" => "The Index, Revised"
then: [
size: 80x24 -->
```screen
▼ [
│ ▽ {
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
── 1 change in version 3 · ] newer · } newest · d next change ──────────────────
  + [1].tags[1]  "shelving"
 14:00:05 ○───────────────○──────────────────────────●───────○ 14:06:49  3 of 4
 books.json .                            json · watching · version 3 of 4 · 1:1
```

The third book's title is `"The Index"` again, because version 3 is the file before your last save. The status bar turns yellow and says `version 3 of 4`, so you can tell that the file on disk is newer than the screen. The marks and the note show version 3's own change: the tag you added.

## Go to the oldest version

To show the oldest version aless keeps, press `{`:

<!-- screen: aless books.json
edit: books.json: "year": 1999, => "year": 2001,
edit: books.json: ["furniture"] => ["furniture", "shelving"]
edit: books.json: "The Index" => "The Index, Revised"
then: [ {
size: 80x24
rows: 21-23 -->
```screen
── Version 1 of 4, opened 14:00:05 · ] newer · } newest · d next change ────────
 14:00:05 ●───────────────○──────────────────────────○───────○ 14:06:49  1 of 4
 books.json .                            json · watching · version 1 of 4 · 1:1
```

Version 1 is the file as you opened it, so it has no changes to mark. The note says when aless opened the file. `]` shows the version after the one on screen, and `}` shows the newest.

## Save while you look back

Stay on version 1. In your editor, change the first book's year again, from `2001` to `2002`, and save the file:

<!-- screen: aless books.json
edit: books.json: "year": 1999, => "year": 2001,
edit: books.json: ["furniture"] => ["furniture", "shelving"]
edit: books.json: "The Index" => "The Index, Revised"
then: [ {
edit: books.json: "year": 2001, => "year": 2002,
size: 80x24
rows: 21-24 -->
```screen
── Version 1 of 5, opened 14:00:05 · ] newer · } newest · d next change ────────
 14:00:05 ●─────────○────────────────○────○──────────────────○ 14:10:55  1 of 5
 books.json .                            json · watching · version 1 of 5 · 1:1
books.json changed at 14:10:55: version 5 is the newest. } shows it
```

aless keeps the new save as version 5, and leaves version 1 on screen, so a save never pulls the screen away from what you are reading. The message line says which version is the newest. To show it, press `}`.

## Save a file that doesn't parse

Press `}` to show the newest version. Then, in your editor, type a second comma after `2002`, and save the file:

<!-- screen: aless books.json
edit: books.json: "year": 1999, => "year": 2001,
edit: books.json: ["furniture"] => ["furniture", "shelving"]
edit: books.json: "The Index" => "The Index, Revised"
then: [ {
edit: books.json: "year": 2001, => "year": 2002,
then: }
edit: books.json: "year": 2002, => "year": 2002,,
size: 80x24
rows: 10-24 -->
```screen
      title: "Shelves and How to Fill Them"
      author: "B. Bookworm"
── parse error · ! shows the full report ───────────────────────────────────────
[tabnas/unexpected]: unexpected character(s): ,
  --> books.json:5:18
  3 |     "title": "A Book of Examples",
  4 |     "author": "A. N. Author",
  5 |     "year": 2002,,
                       ^ unexpected character(s): ,
  6 |     "tags": ["examples", "reference"]
  7 |   },
  …
 14:00:05 ○────────○──────────────○───○─────────────────●────! 14:12:11  5 of 5
 books.json .          !5:18: unexpected character(s): ,  json · watching · 1:1
books.json: 5:18: unexpected character(s): , (! shows the report)
```

The file doesn't parse, so aless keeps version 5 on screen and docks the parser's report beneath it. The timeline marks the failed save with a `!`. A save that doesn't parse is not a version, because it has no tree to show.

Now take the second comma out again, and save the file:

<!-- screen: aless books.json
edit: books.json: "year": 1999, => "year": 2001,
edit: books.json: ["furniture"] => ["furniture", "shelving"]
edit: books.json: "The Index" => "The Index, Revised"
then: [ {
edit: books.json: "year": 2001, => "year": 2002,
then: }
edit: books.json: "year": 2002, => "year": 2002,,
edit: books.json: "year": 2002,, => "year": 2002,
size: 80x24
rows: 20-24 -->
```screen
── 1 change in version 5 · [ older · d next change ─────────────────────────────
  ~ [0].year  2001 → 2002
 14:00:05 ○────────○──────────────○───○─────────────────●────! 14:12:11  5 of 5
 books.json .                                             json · watching · 1:1
Reloaded books.json (19 nodes)
```

The report has gone, and the `!` stays on the timeline as a record of the failed save. The file is the same as version 5 again, so the save adds no version.

## List the versions

To list every version aless keeps, press `v`:

<!-- screen: aless books.json
edit: books.json: "year": 1999, => "year": 2001,
edit: books.json: ["furniture"] => ["furniture", "shelving"]
edit: books.json: "The Index" => "The Index, Revised"
then: [ {
edit: books.json: "year": 2001, => "year": 2002,
then: }
edit: books.json: "year": 2002, => "year": 2002,,
edit: books.json: "year": 2002,, => "year": 2002,
then: v
size: 80x14 -->
```screen
 1:books.json*   2:books.json history
▽ books.json  5 versions, and a save that did not parse
  ! did not parse  14:12:11  5:18: unexpected character(s): ,
  ▼ version 5  14:10:55  ~1  the newest, on screen
    ~ [0].year  2001 → 2002
  ▷ version 4  14:06:49  ~1
  ▷ version 3  14:05:47  +1
  ▷ version 2  14:02:11  ~1
    version 1  14:00:05



 books.json history version 5                              history · 5 versions
books.json: 5 versions. Enter opens one in a tab of its own
```

The history tab opens beside the file's tab, with the newest version first. Each version holds the changes it made, and the version on screen in the file's tab is unfolded. The save that didn't parse has a row of its own, with the parser's message. `j` and `k` move through the list, and `l` unfolds a version to show its changes.

## Open an old version

Version 2 is the file after your first save, with the year changed. To open version 2, press `G` to go to the last row, press `k` to move up to version 2, and press `Enter`:

<!-- screen: aless books.json
edit: books.json: "year": 1999, => "year": 2001,
edit: books.json: ["furniture"] => ["furniture", "shelving"]
edit: books.json: "The Index" => "The Index, Revised"
then: [ {
edit: books.json: "year": 2001, => "year": 2002,
then: }
edit: books.json: "year": 2002, => "year": 2002,,
edit: books.json: "year": 2002,, => "year": 2002,
then: v G k Enter
size: 80x14 -->
```screen
 1:books.json*   2:books.json history   3:books.json v2
▼ [
└─▽ {
  │   title: "A Book of Examples"
  │   author: "A. N. Author"
  └─~ year: 2001
    ▽ tags: [
        "examples"
        "reference"
  ▽ {
── 1 change in version 2 · d next change · v history ───────────────────────────
  ~ [0].year  1999 → 2001
 books.json v2 .                                         json · version 2 · 1:1
Version 2 of books.json, saved 14:02:11, in a tab of its own
```

Version 2 opens in a tab of its own, named `books.json v2`. The tab isn't watched, so it keeps version 2 however the file changes. Its marks show version 2's own change, the year. To go back to the history tab, press `v`.

## Keep the old version

To keep version 2 as a file, type `:ws books-v2.json` in its tab, and press `Enter`. aless writes the text of version 2 as it was saved, and says `Wrote books-v2.json` on the message line. `:w` writes the tree of the version as JSON instead.

To quit, type `:qa` and press `Enter`.

## Go further

You have saved a file, stepped back through its versions, listed them, and kept one. For more information about the keys and commands for versions, see [Compare the versions of a file](/how-to/versions.html). For more information about why aless keeps the text of each version and not its tree, see [How a reload keeps your place](/explanation/reloading.html).

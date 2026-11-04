---
title: Open several files in tabs
description: Open several files at once, each in a tab of its own, move between them, and open and close tabs from inside aless.
group: Files and formats
order: 9
---
Every file named on the command line opens in a tab of its own, and so does a file opened from inside aless. A strip above the tree names the tabs whenever there is more than one. The examples open [`books.json`](/examples/books.json) and [`bookshelf-openapi.yaml`](/examples/bookshelf-openapi.yaml).

## Open files in tabs

Name the files, in any mix of formats:

```sh
aless books.json bookshelf-openapi.yaml
```

<!-- screen: aless books.json bookshelf-openapi.yaml
size: 80x13 -->
```screen
 1:books.json*   2:bookshelf-openapi.yaml*
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
 books.json .                                             json · watching · 1:1
```

The strip numbers the tabs and names their files, with the active tab drawn bright. A mark after a name is the tab's state: `*` for a tab watching its file, `!` for one whose file does not parse, and `✗` for one whose file has gone. Standard input piped into aless opens in a tab named `(stdin)`.

## Move between tabs

`Tab` goes to the next tab and `Shift-Tab` to the previous one, round the end either way. `:tab N` goes to tab N, `:tabnext` and `:tabprev` do what the keys do, and `:tabfirst` and `:tablast` go to the ends. Each tab keeps its own focus, folds, mode and search, so a tab you come back to is as you left it.

## Open more from inside aless

`:open PATH` opens a file in a new tab, read as its extension says, and `:open PATH FORMAT` reads it as the format you name; `:e` is the same command. A directory opens in the [file explorer](/how-to/explore.html), where `Enter` on a file opens it in a new tab, and `:explore DIR` opens a directory the same way.

## Close tabs and quit

`q`, `:q` or `:close` closes the active tab, and closing the last one quits. `:qa`, `:quit` and `:exit` quit with every tab open, and so does `C-c`.

[Choose the format aless reads](/how-to/formats.html) says how to read a tab again as another format, and [Save a document as JSON](/how-to/json.html) how to write one out. The [keys](/reference/keys.html#tabs-and-files) list every tab key and command.

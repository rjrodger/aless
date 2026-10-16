---
title: Open several files in tabs
description: Open files in tabs, move between them, open more from inside aless, read a tab as another format, and write a tab's document out as JSON.
group: In the viewer
order: 5
---
Every file named on the command line opens in a tab of its own, and so does a file opened from inside aless. A strip above the tree names the tabs whenever there is more than one. The examples open [`books.json`](/examples/books.json) and [`bookshelf-openapi.yaml`](/examples/bookshelf-openapi.yaml).

## Open files in tabs

Name the files, in any mix of formats:

```sh
aless books.json bookshelf-openapi.yaml
```

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

The strip numbers the tabs and names their files, with the active tab bright. A mark after a name is the tab's state: `*` for a tab that is watching its file, `!` for one whose file does not parse, and `✗` for one whose file has gone. Standard input piped into the viewer opens as a tab named `(stdin)`.

## Move between tabs

`Tab` goes to the next tab and `Shift-Tab` to the previous one, round the end. `:tab N` goes to tab N, and `:tabnext` and `:tabprev` are the same as the keys. Each tab keeps its own focus, folds, mode and search, so coming back to a tab finds it as you left it.

## Open more from inside aless

`:open PATH` opens a file in a new tab, read as its extension says, and `:open PATH FORMAT` reads it as the format you name; `:e` is the same command. A directory opens in the [file explorer](/how-to/explore.html), where `Enter` on a file opens it in a new tab, and `:explore DIR` opens a directory the same way.

## Close tabs and quit

`q`, `:q` or `:close` closes the active tab, and closing the last one quits. `:qa`, `:quit` and `:exit` quit with every tab open, and so does `C-c`.

## Read a tab as another format

`:format FORMAT` parses the tab's text again as FORMAT, for a file whose extension misled aless: `:format yaml` on a `.conf` file that holds YAML. A parse that fails leaves the document as it was, and `!` shows that attempt's report until the next reload. `r` or `:reload` reads the file from disk again, whether or not the tab is watching it ([Follow a file as it changes](/how-to/watch.html)).

## Write a tab's document out

`:w FILE` writes the tab's document to FILE as JSON, whatever format it was read from, with the numbers as their values; it refuses a file that exists, and `:w! FILE` writes over one. The result is what `aless --json` prints for the same file, so a YAML or TOML file becomes JSON in one command without leaving the viewer. For any other format, or for a value below the root, the command line does it: [Convert a file to another format](/how-to/convert.html). The [keys](/reference/keys.html#tabs-files-and-watching) list every tab command.

---
title: Open several files in tabs
description: Open several files at once, each in a tab of its own, move between the tabs, and open and close tabs from inside aless.
group: Files and formats
order: 9
---
Every file named on the command line opens in a tab of its own. A file you open from inside aless gets a tab of its own too. When more than one tab is open, a strip above the tree names the tabs, so you can see which file each tab holds. The examples on this page open [`books.json`](/examples/books.json) and [`bookshelf-openapi.yaml`](/examples/bookshelf-openapi.yaml).

## Open files in tabs

Name the files on the command line. The files can be in any mix of formats, as in this command:

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

Each entry in the strip is a tab's number and its file's name. aless draws the active tab bright. A mark after the name shows the tab's state: `*` for a tab watching its file, `!` for a tab whose file doesn't parse, and `✗` for a tab whose file has gone. Standard input piped into aless opens in a tab named `(stdin)`.

## Move between tabs

`Tab` goes to the next tab, and `Shift+Tab` goes to the previous one. Both keys wrap round at the ends of the strip, so `Tab` on the last tab goes to the first.

`:tab N` goes to tab N, where N is the tab's number in the strip. `:tabnext` and `:tabprev` do what `Tab` and `Shift+Tab` do. `:tabfirst` and `:tablast` go to the first tab and the last.

Each tab keeps its own focus, folds, mode, and search, so a tab you come back to is as you left it.

## Open files and directories from inside aless

`:open PATH` opens the file at PATH in a new tab. aless reads the file as its extension says. With a format's name after the path, `:open PATH FORMAT` reads the file as that format instead. `:e` is the same command as `:open`.

When PATH is a directory, `:open` opens the directory in the [explorer](/how-to/explore.html). In the explorer, `Enter` on a file opens that file in a new tab. `:explore DIR` opens the directory at DIR in the explorer too.

## Close tabs and quit

`q`, `:q`, or `:close` closes the active tab. Closing the last tab quits aless. `:qa`, `:quit`, and `:exit` quit however many tabs are open, so you don't have to close the tabs one by one. `Ctrl+C` quits the same way.

For more information, see these pages:

- [Choose the format aless reads](/how-to/formats.html). Covers reading a tab again as another format.
- [Save a document as JSON](/how-to/json.html). Covers writing a tab's document to a file.
- [Keys](/reference/keys.html#tabs-and-files). Lists every tab key and command.

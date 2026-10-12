---
title: Choose how the viewer draws a document
description: Data mode and line mode, line numbers, indentation, scrolling, the fold markers, colour and the mouse, set from the command line or inside aless.
group: In the viewer
order: 3
---
The viewer's defaults are jless's: data mode, no line numbers, two spaces of indentation, and three rows kept between the focus and the edge of the screen. Each has an option on the command line and a command inside aless. The examples open [`books.json`](/examples/books.json).

## Data mode and line mode

Data mode shows the tree with what a reader does not need taken out: a key is unquoted when it is a plain word, there are no commas and no closing brackets, and a folded container shows how many items it holds and a preview of the first of them. Line mode shows every line of the document as JSON prints it, quoted keys, commas and closing brackets included, and `%` jumps between a container's opening and closing brackets. `m` switches between the two, and `--mode line` (or `-m line`) starts in line mode:

```screen
▼ [
  ▽ {
      "title": "A Book of Examples",
      "author": "A. N. Author",
      "year": 1999,
    ▽ "tags": [
        "examples",
        "reference"
      ]
    },
  ▽ {
 books.json .                                      json · line · watching · 1:1
```

The status bar says `line` while line mode is on, and `:mode data` or `:mode line` sets it by name. `s` is a third view, the source as it is on disk, with the focused node's line marked ([Look around a file in the viewer](/tutorials/viewer.html)).

## Line numbers

`-n` (`--line-numbers`) numbers the rows on screen, so a count before `g` or `G`, or `:N`, goes to the row you can see the number of. `-r` (`--relative-line-numbers`) shows each row's distance from the focused row instead, which is the count a `j` or a `k` needs to reach it:

```screen
 4 ▽ [
 3   ▽ {
 2       title: "A Book of Examples"
 1       author: "A. N. Author"
 0       year: 1999
 1     ▽ tags: [
 2         "examples"
 3         "reference"
 4   ▽ {
 5       title: "Shelves and How to Fill Them"
 6       author: "B. Bookworm"
 books.json [0].year                                      json · watching · 5:5
```

With both on, the focused row shows its own number and every other row its distance, as vim does. `-N` and `-R` turn each off, for an alias that turned it on. Inside aless, `:set number`, `:set nonumber` and `:set number!` set, clear and toggle the row numbers, and `:set relativenumber`, `:set norelativenumber` and `:set relativenumber!` the relative ones; vim's short forms, `nu` and `rnu`, work too. The numbers count rows, in either mode; the line and column of the focused node in the file are at the right of the status bar, whatever the numbers say.

## Indentation and long values

`--indent N` sets the spaces per level of nesting, two unless you say otherwise, and the JSON the command line prints uses the same. Inside aless, `<` takes one space off every level and `>` adds one, and `:set indent=N` (or `sw=N`) sets it, up to 16. A value longer than the screen is cut at the right edge with an ellipsis: `.` and `,` scroll it one character right and left, and `;` to its end and back again.

## Scrolling and the focus

`--scrolloff N` keeps N rows between the focused row and the top or bottom of the screen, three unless you say otherwise; a large value keeps the focus in the middle of the screen, except at the start and the end of the document. `:set so=N` sets it inside aless. `C-e` and `C-y` scroll one row without moving the focus until it would leave the screen, `C-d` and `C-u` half a page, where a count sets the distance and the next `C-d` remembers it, and `zz`, `zt` and `zb` put the focused row at the centre, the top and the bottom.

## Folding at the start

`--depth N` opens a document with every container deeper than N levels folded, which is how a large document opens as an outline, and `:depth N` folds it again that way at any time. From there `l`, `Space`, `e` and `E` unfold, and `h`, `c` and `C` fold ([Keys](/reference/keys.html#folding)).

## The fold markers, colour and the mouse

The markers `▼ ▽ ▶ ▷` need a font that has them; `--ascii`, or `:set ascii` inside aless, draws `v` and `>` instead, and `:set noascii` goes back. `--no-color`, or `NO_COLOR` in the environment, draws without colour. The mouse wheel scrolls and a click focuses a row, and a pane when there are several; `--no-mouse` leaves the mouse to the terminal, for its own selection. [Run aless on Linux, macOS and Windows](/how-to/platforms.html) has the terminals these matter on, and the [viewer options](/reference/command-line.html#viewer-options) list every option here.

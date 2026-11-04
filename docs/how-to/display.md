---
title: Choose how the viewer draws a document
description: Data mode and line mode, row numbers, indentation, scrolling, folding at the start, the fold markers, colour and the mouse, set when aless starts or inside it.
group: Reading a document
order: 6
---
Each way of drawing a document has an option on the command line, for when aless starts, and a key or a command inside aless. The defaults are data mode, no row numbers, two spaces of indentation, and three rows kept between the focus and the edge of the screen. The examples open [`books.json`](/examples/books.json).

## Data mode and line mode

Data mode shows the tree with the punctuation taken out: a key that is a plain word is unquoted, there are no commas and no closing brackets, and a folded container shows how many items it holds and a preview of the first of them. Line mode shows every line of the document as JSON prints it, quoted keys, commas and closing brackets included. `m` switches between the two, and `--mode line` (or `-m line`) starts in line mode:

<!-- screen: aless --mode line books.json
size: 80x13 -->
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

The status bar says `line` while line mode is on. In line mode, `%` goes between a container's opening and closing brackets, and `:mode data` or `:mode line` sets the mode by name.

`s` is a third way to look: the source view, the text aless read, in place of the tree, with the focused node's line marked. Any key but the scrolling ones, `:` and `r` goes back to the tree.

## Row numbers

`-n` (`--line-numbers`) numbers the rows on screen, so a count before `g` or `G`, or `:N`, goes to the row you can see the number of. `-r` (`--relative-line-numbers`) numbers each row by its distance from the focused row instead, which is the count a `j` or a `k` needs to reach it:

<!-- screen: aless -r books.json
keys: 4j
size: 80x13 -->
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

With both on, the focused row shows its own number and every other row its distance, as vim does. `-N` and `-R` turn each off again, for when a shell alias turned it on. Inside aless, `:set number`, `:set nonumber` and `:set number!` set, clear and toggle the row numbers, and `:set relativenumber`, `:set norelativenumber` and `:set relativenumber!` the distances. vim's short forms, `nu` and `rnu`, work too.

The numbers count rows in either mode. The line and column where the focused node starts in the file are at the right of the status bar, whatever the numbers say.

## Indentation and long values

`--indent N` sets the spaces a level of nesting takes, two unless you say otherwise. Inside aless, `<` takes a space off every level and `>` adds one, and `:set indent=N` (or `sw=N`) sets it, up to 16.

A value longer than the screen is cut at the right edge with an ellipsis. `.` and `,` scroll it a character right and left, and `;` scrolls it to its end and back to its start.

## Scrolling and the focus

`--scrolloff N` keeps N rows between the focused row and the top or the bottom of the screen, three unless you say otherwise, and `:set so=N` sets it inside aless. A large value keeps the focus in the middle of the screen, except at the start and the end of the document.

`C-e` and `C-y` scroll a row at a time, and move the focus only to keep that many rows between it and the edge. `C-d` and `C-u` move half a page; a count before them sets the distance, and later presses keep it. `zz`, `zt` and `zb` put the focused row at the centre, the top or the bottom of the screen.

## Folding at the start

`--depth N` opens a document with only its first N levels open: a container N levels below the root starts folded, and so does everything in it. That is how a large document opens as an outline. `:depth N` folds the document that way again at any time. From there, `l`, `Space`, `e` and `E` unfold, and `h`, `c` and `C` fold ([Keys](/reference/keys.html#folding-and-views)).

## The fold markers, colour and the mouse

The markers `▼ ▽ ▶ ▷` need a font that has them. `--ascii`, or `:set ascii` inside aless, draws `v` and `>` instead, and `x` for a file that has gone; `:set noascii` goes back.

`--no-color`, or `NO_COLOR` in the environment, draws without colour. In colour, the source view is coloured by what the tab's grammar read: strings green, numbers magenta, keywords yellow, keys and names blue, types and macros cyan, and comments grey. A text over 512 KiB, plain text, and a grammar in ABNF, EBNF or GBNF stay plain.

The mouse wheel scrolls three rows a notch, and a click focuses the row under it. `--no-mouse` leaves the mouse to the terminal, for its own selection. [Run aless on Linux, macOS and Windows](/how-to/platforms.html) has the terminals these matter on, and the [viewer options](/reference/command-line.html#viewer-options) list every option on this page.

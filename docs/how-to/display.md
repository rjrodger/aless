---
title: Choose how the viewer draws a document
description: Choose between data mode and line mode, number the rows, set the indentation and the scrolling, fold a document as it opens, and change the fold markers, the colour, and the mouse, when aless starts or from inside it.
group: Reading a document
order: 6
---
You can set each way of drawing a document in two places: with an option on the command line when aless starts, and with a key or a command inside aless. The defaults are data mode, no row numbers, 2 spaces of indentation, and 3 rows kept between the focus and the edge of the screen. The examples open [`books.json`](/examples/books.json).

## Data mode and line mode

Data mode shows the tree without its punctuation: a key that is a plain word has no quotes, and there are no commas and no closing brackets. A folded container shows how many items it holds and a preview of the first of them. Line mode shows every line of the document as JSON prints it, with the quoted keys, the commas, and the closing brackets. `m` switches between the two modes. `--mode line`, or `-m line`, starts aless in line mode:

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

The status bar says `line` while line mode is on. In line mode, `%` moves between a container's opening and closing brackets. `:mode data` and `:mode line` set the mode by name.

`s` opens the source view. Beside data mode and line mode, the source view is a third way to look at the document. In place of the tree, it shows the text aless read. aless marks the focused node's line, so you can find that node in the text. Any key other than the scrolling keys, `:`, and `r` goes back to the tree.

## Row numbers

`-n`, or `--line-numbers`, numbers the rows on screen. A count before `g` or `G` then goes to the row that shows that number. So does `:N`, where N is the row's number. `-r`, or `--relative-line-numbers`, numbers each row by its distance from the focused row instead. That distance is the count a `j` or a `k` needs to reach the row:

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

The focused row, `year`, shows `0`. The `title` row, two rows up, shows `2`, so `2k` reaches that row.

With `-n` and `-r` both on, the focused row shows its own number and every other row shows its distance, as vim does. `-N` and `-R` turn each off again, for when a shell alias has turned one on. Inside aless, `:set number`, `:set nonumber`, and `:set number!` set, clear, and toggle the row numbers. `:set relativenumber`, `:set norelativenumber`, and `:set relativenumber!` do the same for the distances. The short forms vim uses, `nu` and `rnu`, work too.

The numbers count rows in data mode and in line mode alike. The line and column where the focused node starts in the file are at the right of the status bar, whatever the numbers say.

## Indentation and long values

`--indent N` sets how many spaces a level of nesting takes. The default is 2. Inside aless, `<` takes a space off every level and `>` adds one. `:set indent=N`, or `:set sw=N`, sets the count. Whichever way you set the count, it is at most 16.

When a value is longer than the screen, aless cuts the value at the right edge. An ellipsis marks the cut. `.` scrolls the value a character to the right, and `,` a character to the left. `;` scrolls the value to its end and back to its start.

## Scrolling and the focus

`--scrolloff N` keeps N rows between the focused row and the top or the bottom of the screen. The default is 3. Inside aless, `:set so=N` sets the same count. A large value keeps the focus in the middle of the screen, except at the start and the end of the document.

`Ctrl+E` and `Ctrl+Y` scroll a row at a time. They move the focus only to keep the `--scrolloff` count of rows between the focus and the edge. `Ctrl+D` and `Ctrl+U` move half a page. A count before either sets the distance, and later presses keep it. `zz`, `zt`, and `zb` put the focused row at the centre, the top, or the bottom of the screen.

## Folding at the start

`--depth N` opens a document with only its first N levels open. A container N levels below the root starts folded, and so does everything in it. That's how a large document opens as an outline. `:depth N` folds the document that way again at any time. From there, `l`, `Space`, `e`, and `E` unfold, and `h`, `c`, and `C` fold. For more information, see [Keys](/reference/keys.html#folding-and-views).

## Fold markers, colour, and the mouse

The fold markers `▼ ▽ ▶ ▷` need a font that has them. `--ascii`, or `:set ascii` inside aless, draws `v` and `>` instead. Either one also draws `x` for a file that has gone. `:set noascii` brings the markers back.

`--no-color`, or `NO_COLOR` in the environment, draws without colour. When aless draws in colour, it colours the source view by what the tab's grammar read. Strings are green, numbers magenta, keywords yellow, keys and names blue, types and macros cyan, and comments grey. A text over 512 KB, plain text, and a grammar file in ABNF, EBNF, or GBNF stay uncoloured.

The mouse wheel scrolls three rows a notch. A click focuses the row under the pointer. `--no-mouse` leaves the mouse to the terminal, for the terminal's own selection.

For more information about which terminals these settings matter on, see [Run aless on Linux, macOS, and Windows](/how-to/platforms.html). The section on viewer options in the command-line reference lists every option on this page. For more information, see [Command line](/reference/command-line.html#viewer-options).

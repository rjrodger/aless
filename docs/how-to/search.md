---
title: Search a document
description: Find a value, a key, or a pattern with a regular expression, move between the matches, and learn which text the search reads.
group: Reading a document
order: 4
---
`/` starts a search, as it does in less, vim, and jless. The pattern is a regular expression. A fold hides nothing from the search: aless reads the rows inside a fold. When a match sits inside a fold, aless unfolds whatever hides the match.

The examples on this page open [`books.json`](/examples/books.json).

## Search forward and back

Press `/`, type a pattern, and press `Enter`:

<!-- screen: aless books.json
keys: / Index Enter
size: 80x14 -->
```screen
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
 books.json [2].title                                    json · watching · 15:5
/Index  [1/1]
```

The focus moves to the first match after it. The status bar gives the match's path, its line, and its column in the file. The status line shows the pattern and the count: the current match's number out of the number of matches, here `[1/1]`.

`n` goes to the next match, and `N` to the previous one. Both wrap round the end of the document, so `n` on the last match goes to the first match. The count shows `W` after a wrap. `?` searches backward from the focus. `n` and `N` then keep that direction: `n` goes backward, and `N` forward.

To search for the last pattern again, press `/` and then `Enter`, with no pattern between them. `Esc` leaves the prompt without searching.

## Search for the key under the focus

With the focus on a key, `*` searches forward for the next node with the same key. `#` searches backward for the same key. The next `year` is one keystroke away, however deep in the document it sits.

## Write a pattern

A pattern is a regular expression in the syntax of the `regex` crate for Rust. For more information, see the [`regex` syntax reference](https://docs.rs/regex/latest/regex/#syntax) on `docs.rs`. When aless reads a pattern, it applies three rules that suit a document:

- A pattern with no capital letter ignores case, so `/index` finds `Index`. A pattern with a capital letter minds case, so `/Index` finds only `Index`. `/s` at the end of a pattern minds case either way, so `/index/s` finds `index` and not `Index`.
- `[`, `]`, `{`, and `}` match themselves, because documents are full of them. So `/[]` finds an empty array, and `/{}` finds an empty object. Escape one to write a character class or a count: `/\[bch\]at` finds `bat`, `cat`, and `hat`. `/(ha)\{2,3\}` finds `haha` and `hahaha`.
- aless drops a single `/` at the end of a pattern, so `/s/` searches for `s`. To find a `/` at the end, write two: `/a//` finds `a/`.

When the regular-expression engine refuses a pattern, such as one with an unclosed group, the search finds nothing. The status line shows the engine's message.

## Match a row's text

The search reads each row's text as [line mode](/how-to/display.html#data-mode-and-line-mode) shows it: the key in quotes, a colon, and the value. So `/"year": 2012` finds the second book, and `/year: 2012` finds nothing. A container's row is its key and its opening bracket, as in `"tags": [`. An empty container's row is its key and both brackets, as in `"tags": []`.

The search reads the rows inside a fold as well. When a match sits inside a fold, aless unfolds whatever hides the match.

A row holds one node, so a pattern can't span two rows. Search for a value, a key, or a key with its value. A pattern that needs two elements of an array at once matches no row.

In the [explorer](/how-to/explore.html), `/` searches the entries listed so far. There the search reads each row's name, size, and format.

The reference lists every search key. For more information, see [Keys](/reference/keys.html#search).

The search in jless reads the whole document as one line. The search in aless reads each row on its own. For more information, see [aless and jless](/explanation/jless.html).

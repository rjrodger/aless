---
title: Search a document
description: Find a value, a key or a pattern with a regular expression, move between the matches, and know which text the search reads.
group: Reading a document
order: 4
---
`/` starts a search, as it does in less, vim and jless. The pattern is a regular expression, and aless unfolds whatever hides a match. The examples open [`books.json`](/examples/books.json).

## Search forward and back

Press `/`, type a pattern and press `Enter`:

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

The focus moves to the first match after it, and the status bar gives the match's path and its line and column in the file. The row below shows the pattern, and which match this is of how many. `n` goes to the next match and `N` to the previous one, round the end of the document either way, and the count shows `W` after a wrap. `?` searches backward from the focus, and `n` and `N` then keep that direction. `/` and `Enter` with no pattern searches for the last one again, and `Esc` leaves the prompt without searching.

## Search for the key under the focus

With the focus on a key, `*` searches forward for the next node with the same key, and `#` searches backward. The next `year` is one key away, however deep in the document it sits.

## Write a pattern

A pattern is a regular expression in the syntax of Rust's [regex crate](https://docs.rs/regex/latest/regex/#syntax), with three rules that suit a document:

| Rule | Example |
|---|---|
| A pattern with no capital letter ignores case, and one with a capital letter minds it. `/s` at the end minds case either way. | `/index` finds `Index`; `/Index` finds only `Index`; `/index/s` finds `index` and not `Index` |
| `[ ] { }` match themselves, since documents are full of them. Escape one for a character class or a count. | `/[]` finds an empty array and `/{}` an empty object; `/\[bch\]at` finds `bat`, `cat` and `hat`; `/(ha)\{2,3\}` finds `haha` and `hahaha` |
| A single `/` at the end of a pattern is dropped. To find a `/` at the end, write two. | `/s/` searches for `s`; `/a//` finds `a/` |

A pattern the regex engine refuses, such as an unclosed group, shows its message on the status line and finds nothing.

## What the search reads

The search reads each row's text as [line mode](/how-to/display.html#data-mode-and-line-mode) shows it: the key in quotes, a colon, and the value. So `/"year": 2012` finds the second book, and `/year: 2012` finds nothing. A container's row is its key and its opening bracket, `"tags": [`, and an empty container's row its brackets, `"tags": []`. What a fold hides is searched all the same, and unfolded when it matches.

A row holds one node, so a pattern cannot span two rows. Search for a value, a key, or a key with its value, not for two elements of an array at once.

In the [file explorer](/how-to/explore.html), `/` searches the entries listed so far, each row's name, size and format. The [keys](/reference/keys.html#search) list every search key, and [aless and jless](/explanation/jless.html) says how this search differs from jless's, which reads the whole document as one line.

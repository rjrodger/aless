---
title: Search a document
description: Find a value, a key or a pattern in the viewer with a regular expression, move between the matches, and know which text the search reads.
group: In the viewer
order: 2
---
`/` starts a search in the viewer, as it does in less, vim and jless. The pattern is a regular expression, and aless unfolds whatever hides a match. The examples open [`books.json`](/examples/books.json).

## Search forward and back

Press `/`, type a pattern and press `Enter`:

```screen
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

The focus moves to the first match after it, the status bar gives the match's path and its position in the file, and the row below shows the pattern and which match this is of how many. `n` goes to the next match and `N` to the previous one, round the end of the document either way, and the count shows `W` after a wrap. `?` searches backward from the focus, and `n` and `N` then follow that direction. `Esc` leaves the prompt without searching.

## Search for the key under the focus

With the focus on a key, `*` searches forward for the next node with the same key and `#` backward, so the next `year` is one key away however deep in the document it sits.

## Write a pattern

A pattern is a regular expression in the syntax of Rust's [regex crate](https://docs.rs/regex/latest/regex/#syntax), with three rules that fit it to a document:

- Case. A pattern with no capital letter ignores case, and one with a capital letter minds it: `/index` finds `Index` and `/Index` finds only that. `/s` after the pattern forces case: `/index/s` finds `index` and not `Index`.
- Brackets. `[ ] { }` match themselves, since documents are full of them: `/[1, 2]` finds an array of those two values, and `/{}` an empty object. Escape a bracket for a character class or a count: `/\[bch\]at` finds `bat`, `cat` and `hat`, and `/(ha)\{2,3\}` finds `haha` and `hahaha`.
- Slashes. A single `/` at the end of a pattern is dropped, so `/s/` searches for `s`; to find a `/` at the end, write two: `/a//` finds `a/`.

A pattern the regex engine refuses, such as an unclosed group, shows its message on the status line and finds nothing.

## What the search reads

The search reads each row's text as line mode shows it: the key in quotes, a colon, and the value, so `/"year": 2012` finds the second book and `/year: 2012` finds nothing. A container's row is its key and its opening bracket, `"tags": [`, and an empty container's its brackets, `"tags": []`; what is inside a fold is searched all the same, and unfolded when it matches. A row holds one node, so a pattern cannot span two rows: search for a value, a key, or a key with its value, not for two members at once. The command line's `--find` reads the same text, and prints the matches as entries ([Ask a file questions from a script](/tutorials/scripting.html#find-a-node)).

In the [file explorer](/how-to/explore.html), `/` searches the names listed so far. The [keys](/reference/keys.html#search) list every search key, and [aless and jless](/explanation/jless.html) says how this search differs from jless's, which reads the whole document as one line.

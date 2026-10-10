---
title: Where each value came from
description: How aless knows the line and column of every value without any grammar recording it, where that is exact, and where it is a best effort.
order: 2
---
Every entry aless prints has a `line` and a `col`, the status bar shows them for the focused node, the source view opens at them, and a reload falls back on them. None of the grammars records them. aless works them out after the parse.

## Matching the tokens to the tree

The parsing engine reports every token it reads, with the token's position, as it reads it. The parse also produces the tree. aless walks the two side by side: the keys and the leaf values of the tree, in document order, against the tokens that carry a value, in source order. Where a key or a value matches the next such token, it takes that token's position. The search looks a bounded distance ahead, so a token the tree has no node for does not throw the alignment off.

The benefit is that position is a property of every format at once, including a grammar written on the command line, rather than a feature each grammar has to remember to build.

## Exact, and best effort

The alignment is exact wherever the tree's values are the tokens' values: the JSON family, TOML, INI, CSV and TSV, and ZON. It is a best effort where a grammar builds values the source does not spell out. Markdown's syntax tree, XML's element records, CSS's and PGN's trees, an expression's operators and parts of YAML make nodes whose text is not one token's text, and a value assembled from several tokens matches none of them. A `.proto` file's descriptor is the loosest: it is derived from the file rather than read from it, its statements regrouped and its labels, types and indexes made by the reader, so only its first values, the package and the imports, are placed reliably.

A node the alignment cannot place has no position, and says so: its `line` and `col` are `null`. It does not inherit its parent's, since a wrong position is worse than none for a caller that jumps to it.

## The other direction

`--where --at LINE:COL` asks the opposite question, which node a position is in. aless answers with the node that starts last at or before the position on its line, the innermost of several starting there, so a column inside a long value names that value. A line with no node of its own, a comment or a closing bracket, answers with the last node before it. A position outside the text, past the last line or the end of a line, names nothing, and the error says so.

Those rules are what let a linter's or a test's `42:7` become a path: [Ask a file questions from a script](/tutorials/scripting.html) shows one in use.

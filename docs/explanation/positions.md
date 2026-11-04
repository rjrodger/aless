---
title: Where each value came from
description: How aless knows the line and column of every value without most grammars recording them, where that is exact, and where it is a best effort.
order: 2
---
The status bar shows the line and column where the focused node starts, the source view opens there, and a reload falls back on them to find your place. Almost no grammar records them. aless works them out after the parse.

## Matching the tokens to the tree

The parsing engine reports every token it reads, with the token's position, as it reads it, and the parse produces the tree. aless walks the two side by side: the keys and the leaf values of the tree, in document order, against the tokens that carry a value, in source order. Where a key or a value matches the next such token, it takes that token's position. The match looks a bounded distance ahead, so a token the tree has no node for does not throw the rest off.

So position is a property of every format at once, a grammar written on the command line included. No grammar has to remember to build it.

## Exact, and best effort

The match is exact wherever the tree's values are the tokens' values: the JSON family, TOML, INI, CSV, TSV and ZON. It is a best effort where a grammar builds values the source does not spell out. Markdown's syntax tree, XML's element records, the trees of CSS and PGN, an expression's operators and parts of YAML hold nodes whose text is no one token's text, and a value put together from several tokens matches none of them.

Two formats take another route. A `.proto` file's descriptor is derived from the file rather than read from it, its statements regrouped and its labels, types and indexes made by the reader, so only its first values, the imports, are placed reliably. A C syntax tree is the one tree that records where each of its tokens is, so aless reads the places from the tree itself, exactly, each time the tree repeats a token. A grammar's spec is written by its compiler, which keeps no places, so none of its nodes has one.

A node that cannot be placed has no position, and says so: the status bar shows none for it. It does not borrow its parent's, since a wrong position is worse than none for a source view that opens at it. [Formats](/reference/formats.html#source-positions) lists how exact each format is.

## The other direction

A reload asks the opposite question: which node is at a position? When the node you were reading is gone from the new document, aless looks within its nearest surviving ancestor for the node closest to the old line, so that a renamed key or a reordered entry keeps your place. The positions are one half of that; [How a reload keeps your place](/explanation/reloading.html) tells the other.

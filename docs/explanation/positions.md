---
title: Where each value came from
description: How aless finds the line and column of every value when almost no grammar records them, where the position is exact, and where it is a best effort.
order: 2
---
The status bar shows the line and column where the focused node starts. The [source view](/how-to/display.html#data-mode-and-line-mode) opens at that line and column. A reload falls back on that position to find your place when the node you were reading has gone from the new document. Almost no grammar records a position. Instead, aless finds every position after the parse.

## Matching the tokens to the tree

The parsing engine reports every token as the engine reads it. Each report includes the token's position. The parse itself produces the tree. aless walks the two side by side: the keys and the leaf values of the tree, in document order, against the tokens that carry a value, in source order. Where a key or a value matches the next such token, the node takes that token's position. The match looks a bounded distance ahead, so a token the tree has no node for does not misplace the nodes after it.

Position is therefore a property of every format at once. A custom grammar written on the command line gets positions like any built-in grammar. No grammar has to build them.

## Placing a node exactly, or as a best effort

The match is exact wherever the tree's values are the tokens' values. The tree's values are the tokens' values in the JSON family, TOML, INI, CSV, TSV, and ZON. The match is a best effort where a grammar builds values the source does not spell out. Markdown's syntax tree, XML's element records, the trees of CSS and PGN, an expression's operators, and parts of YAML hold nodes whose text is no one token's text. A value built from several tokens matches none of them.

Two formats are exceptions. For a `.proto` file, aless derives the descriptor rather than reading it from the file: aless regroups the file's statements and makes the labels, the types, and the indexes itself. So only the descriptor's first values get a reliable position. Those first values are the imports. A C syntax tree is the one tree that records where each of its tokens is. Each time the tree repeats a token, aless reads that token's exact position from the tree itself.

A grammar's spec comes from its compiler. The compiler keeps no positions, so none of the spec's nodes has a position.

A node that aless cannot place has no position. The status bar shows none for it. The node does not borrow its parent's position, because a wrong position is worse than none for a source view that opens at that position. For more information about how exact each format's positions are, see [Formats](/reference/formats.html#source-positions).

## Finding the node at a position

A reload asks the opposite question: which node is at a position? When the node you were reading is gone from the new document, aless looks within that node's nearest surviving ancestor for the node closest to the old line. A renamed key or a reordered entry therefore keeps your place. The positions are one half of that answer. For more information about the other half, see [How a reload keeps your place](/explanation/reloading.html).

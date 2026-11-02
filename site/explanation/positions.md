---
title: Where each value came from
description: How aless knows the line and column of every value without any grammar recording it, where that is exact, and where it is a best effort.
order: 2
---
The status bar shows the line and column of the focused node, the source view opens at them, and a reload falls back on them. None of the grammars records them. aless works them out after the parse.

## Matching the tokens to the tree

The parsing engine reports every token it reads, with the token's position, as it reads it. The parse also produces the tree. aless walks the two side by side: the keys and the leaf values of the tree, in document order, against the tokens that carry a value, in source order. Where a key or a value matches the next such token, it takes that token's position. The search looks a bounded distance ahead, so a token the tree has no node for does not throw the alignment off.

The benefit is that position is a property of every format at once, including a grammar written on the command line, rather than a feature each grammar has to remember to build.

## Exact, and best effort

The alignment is exact wherever the tree's values are the tokens' values: the JSON family, TOML, INI, CSV and TSV, and ZON. It is a best effort where a grammar builds values the source does not spell out. Markdown's syntax tree, XML's element records, CSS's and PGN's trees, an expression's operators and parts of YAML make nodes whose text is not one token's text, and a value assembled from several tokens matches none of them. A `.proto` file's descriptor is the loosest: it is derived from the file rather than read from it, its statements regrouped and its labels, types and indexes made by the reader, so only its first values, the imports, are placed reliably; its package, which it holds after its lists, comes too late to be placed. A C syntax tree needs no alignment: it records where each token is, so each token and its text are placed exactly there, each time the tree repeats it, and what the reader adds, a node's kind, a token's name and the numbers of a span, has no position. A grammar's spec is written by its compiler, which keeps no places, so none of its nodes has one.

A node the alignment cannot place has no position, and says so: the status bar shows none for it. It does not inherit its parent's, since a wrong position is worse than none for the source view that jumps to it.

## The other direction

A reload asks the opposite question, which node is at a position. When the node you were reading is gone from the new document, aless looks within its nearest surviving ancestor for the node closest to the old source line, so that a renamed key or a reordered entry keeps your place. The source view asks the first question again: `s` opens the text at the focused node's line, with that line marked.

The positions are one half of how a reload finds your place; [How a reload keeps your place](/explanation/reloading.html) tells the other half.

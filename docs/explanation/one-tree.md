---
title: One tree for every format
description: Why aless reads every format into the same kind of tree, what that buys, and what the single model gives up.
order: 1
---
aless reads JSON, YAML, TOML, INI, CSV, XML, Markdown, feeds, stylesheets, chess games, C source, grammars and more, and shows every one of them the same way. It can, because it never shows a format. It shows one kind of value, which every format is read into: objects with their keys in source order, arrays, strings, numbers, `true`, `false` and `null`. That is JSON's data model, and it is all the viewer, the search and the copy commands ever handle.

## What each format becomes

The JSON family maps onto that model as it stands, and so do YAML and TOML, almost entirely. Every other format is given a shape: a CSV file becomes an array of records keyed by its header row, an XML file a tree of element records, a Markdown file its syntax tree, and a `.proto` file the descriptor protoc writes for it. [Formats](/reference/formats.html) gives the shape of each.

Some of those shapes are verbose, XML's and Markdown's above all, and the viewer shows them as they are rather than prettifying them back towards the source. A prettified view would be a second model, with its own keys, its own paths and its own search, for one format. When the tree's shape gets in the way, `s` shows the source text in its place.

## What the model buys

Everything above the parser is written once. There is one key map, one way to search, one set of path forms for the paths you copy, one error report and one way of knowing where a value came from, for every format. That includes formats aless was never told about: a grammar you name on the command line gets the viewer, the search and the source positions without aless knowing anything about the format it reads.

## What the model gives up

A model every format fits in holds only what they have in common. Comments are not part of it, nor are YAML's anchors and tags, and neither is a number's type.

Numbers are the loss you are most likely to meet. The model's numbers are 64-bit floats, as JavaScript's are, so an integer beyond 2^53 is held as the nearest float, and `1.0` and `1` are the same value. aless keeps a number's spelling beside its value, from the token it was read from, so the viewer, the search, `:w` and the copy commands give `1.0`, `1e21` and a long integer as the source spells them, as jless does. The spelling is kept only where it is also JSON's, since `:w` and the copy commands write JSON. `0x1F` in JSON5 and `1_000` in TOML are given as the values they stand for, `31` and `1000`, and the source view shows the spelling.

## Why one engine

Every format is parsed by the same engine, [tabnas](https://tabnas.dev), with a grammar for each format, and aless has no parser of its own. That is why a new format is a grammar rather than a feature, and why every format reports its errors in the same shape, with a line, a column and a hint.

The cost is speed. A general rule engine is slower than a parser written by hand for one format: aless reads about a megabyte a second, where a dedicated JSON parser reads hundreds. For the files people read in a terminal that is fast enough, and [Large inputs and limits](/explanation/large-inputs.html) says what happens with the files for which it is not.

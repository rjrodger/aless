---
title: One tree for every format
description: Why aless reads every format into the same kind of tree, what that buys, and what the single model gives up.
order: 1
---
aless reads JSON, YAML, TOML, INI, CSV, XML, Markdown, feeds, stylesheets, chess games, C source, grammars, and more. It shows every one of them the same way, because it never shows a format. It shows one kind of value instead. It reads every format into that kind of value. That kind of value is an object, an array, a string, a number, `true`, `false`, or `null`. That is JSON's data model. An object keeps its keys in source order. The viewer, the search, and the copy commands handle that model and nothing else.

## What each format becomes

The JSON family maps onto JSON's data model without change. YAML and TOML map onto it almost entirely. Every other format gets a shape:

- A CSV file becomes an array of records keyed by its header row.
- An XML file becomes a tree of element records.
- A Markdown file becomes its syntax tree.
- A `.proto` file becomes the descriptor protoc writes for it.

For more information about the shape of every format, see [Formats](/reference/formats.html).

Some of those shapes are verbose. XML's and Markdown's are the most verbose of them. The viewer shows each shape as it is, and does not prettify it back towards the source. A prettified view would be a second model for one format, with its own keys, its own paths, and its own search. When the tree's shape gets in the way, press `s` to see the source text in its place.

## What the model buys

Everything built on the parser is written once. For every format there is one key map, one way to search, one set of path forms for the paths you copy, one error report, and one way of knowing where a value came from. That includes formats with no built-in grammar. A custom grammar you name on the command line gets the viewer, the search, and the source positions. Nothing in aless is written for the format that grammar reads.

## What the model gives up

A model every format fits in holds only what the formats have in common. It leaves out comments, YAML's anchors and tags, and a number's type.

Numbers are the loss you are most likely to meet. The model's numbers are 64-bit floats. JavaScript's numbers are 64-bit floats too. Because the model's numbers are floats, the model holds an integer beyond 2^53 as the nearest float. It also holds `1.0` and `1` as the same value.

aless keeps a number's spelling beside its value. It takes the spelling from the number's token in the source. So the viewer, the search, `:w`, and the copy commands give `1.0`, `1e21`, and a long integer as the source spells them. jless does the same.

aless keeps the spelling only where it is also JSON's, because `:w` and the copy commands write JSON. So aless gives `0x1F` in JSON5 as the value it stands for, `31`, and gives `1_000` in TOML as `1000`. The [source view](/how-to/display.html#data-mode-and-line-mode) shows the spelling.

## Why one engine parses every format

One engine parses every format, with a grammar for each format. That engine is [tabnas](https://tabnas.dev). The link goes to the tabnas site, outside this documentation. Because aless has no parser of its own, and because the one engine parses every format from a grammar, a new format is a grammar rather than a feature. For the same reasons, every format reports its errors in the same shape. Each error report gives a line, a column, and a hint.

The cost is speed. A general rule engine is slower than a parser written by hand for one format. aless reads about 1 MB a second, where a dedicated JSON parser reads hundreds of megabytes a second. For the files people read in a terminal, about 1 MB a second is fast enough. For more information about the files that rate is too slow for, see [Large inputs and limits](/explanation/large-inputs.html).

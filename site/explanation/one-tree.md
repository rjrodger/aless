---
title: One tree for every format
description: Why aless reads every format into the same kind of tree, what each format becomes, and what the single model gives up.
order: 1
---
aless reads JSON, YAML, TOML, INI, CSV, XML, Markdown, feeds, stylesheets, chess games and a dozen other formats, and shows every one of them the same way. That is possible because it does not show the formats. It shows one kind of value, which every format is read into: objects with their keys in source order, arrays, strings, numbers, true, false and null. That is JSON's data model, and it is the only thing the viewer, the search, the paths and the command line's outputs ever deal with.

## What each format becomes

The JSON family maps onto that model as it is, and so does most of YAML and TOML. The others are given a shape:

| Format | Becomes |
|---|---|
| CSV, TSV | an array of records, each an object keyed by the header row, every value a string |
| INI | an object of sections, each an object of its keys, every value a string |
| XML | element records: `name`, `localName`, `attributes` and `children`, with text as strings among the children |
| Markdown | its syntax tree: a `type` and `children` for each block and inline node |
| RSS, Atom | one Atom-shaped object, whichever of the two the feed was |
| CSS | its syntax tree: a `type` for each rule, declaration, comment and at-rule |
| `.proto` | its FileDescriptorProto, as protoc writes one in JSON, rather than the syntax its grammar parses |
| PGN | an array of games, each its tags, its moves and its result |
| an expression | its value, each operation an array of its operator and its terms: `1+2*3` is `["+", 1, ["*", 2, 3]]` |
| a version | its `major`, `minor`, `patch`, `prerelease` and `build` |
| plain text | an array of its lines |
| a grammar of yours | what its annotations build |

Some of those shapes are verbose, XML's and Markdown's above all, and the viewer shows them as they are rather than prettifying them back towards the source. `s` shows the source beside the tree whenever the tree's shape gets in the way.

## What the model buys

Everything above the parser is written once. There is one key map, one path syntax (jq's), one search, one error object, one set of exit statuses and one way of reporting where a value came from, for every format, including formats that did not exist when the code was written. A grammar you write on the command line gets the viewer, the outputs and the source positions without aless knowing anything about it.

Conversion falls out of the same design. A document read from any format is a value of the one model, and a format's writer takes a value of that model, so every format aless reads can be written as every format it writes. The writers are told what they cannot keep, and say so ([Convert a file to another format](/how-to/convert.html)).

## What the model gives up

A model every format fits in holds only what they have in common. Comments, YAML's anchors and tags, and the spelling of a number are not part of it.

Numbers are the loss you are most likely to meet. The model's numbers are 64-bit floats, as JavaScript's are, so an integer beyond 2^53 is held as the nearest float, and `1.0` and `1` are the same value. The viewer shows the value, not the spelling, which is one of the ways aless differs from jless; the source view (`s`) shows the spelling, and `--render json` keeps it where the source's spelling is JSON.

## Why one engine

Every format is parsed by the same engine, [tabnas](https://tabnas.dev), with a grammar for each format. aless has no parser of its own. That is why a new format is a grammar rather than a feature, and why every format reports its errors in the same shape, with a line, a column and a hint.

The cost is speed. A general rule engine is slower than a parser written by hand for one format: aless reads about a megabyte a second, where a dedicated JSON parser reads hundreds. For the files people read in a terminal that is fast enough, and [Large inputs, streaming and limits](/explanation/large-inputs.html) says what happens with the files for which it is not.

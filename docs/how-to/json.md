---
title: Save a document as JSON
description: Write the document in a tab to a file as JSON, whatever format it was read from, or copy one value of it as JSON.
group: Reading a document
order: 7
---
Whatever format a tab was read from, its document is a tree of JSON's values, and aless writes it out as JSON. The example opens the [tutorial's file](/examples/bookshelf-openapi.yaml), an OpenAPI document in YAML.

## Write the whole document

Type `:w`, a space and a filename, such as `:w bookshelf.json`, then press `Enter`. aless says `Wrote bookshelf.json` on the status line.

aless writes the whole document, whichever node has the focus, pretty-printed with two spaces a level and its keys in source order. A relative name is taken from the directory aless was started in. `:w` refuses a file that exists, so that a slip of the keyboard does not overwrite one; `:w!` writes over it.

Line mode (`m`) shows the same JSON, with its folds ([Choose how the viewer draws a document](/how-to/display.html)).

## What the JSON holds

The file holds the tree the viewer shows, not the text of the source:

- Comments are not in the tree, so they are not in the file.
- A number keeps its spelling where JSON spells it the same way, so `1.0` stays `1.0`, and a spelling JSON lacks gives the value it stands for, so `0x1F` becomes `31` ([Numbers](/reference/formats.html#numbers)).
- A format whose tree has a shape of its own is written as that shape: an XML file as element records, a Markdown file as its syntax tree, a CSV file as an array of records ([Formats](/reference/formats.html)).

## Copy one value instead

`yy` copies the focused value as indented JSON, and `yv` copies it as JSON on one line. On a container, that is the whole of it, so `yy` on one path of an API description copies every operation under it ([Copy a value or its path](/how-to/copy.html)).

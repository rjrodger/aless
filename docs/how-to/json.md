---
title: Save a document as JSON
description: Write the document in a tab to a file as JSON, whatever format the tab is read from, or copy one value of the document as JSON.
group: Reading a document
order: 7
---
Whatever format a tab is read from, its document is a tree of JSON's values. aless writes that tree out as JSON. The example opens the tutorial's file, [`bookshelf-openapi.yaml`](/examples/bookshelf-openapi.yaml). That file is an OpenAPI document in YAML.

## Write the whole document

To write the document to a file, type `:w`, a space, and a filename, such as `:w bookshelf.json`, and then press `Enter`. aless says `Wrote bookshelf.json` on the status line.

Whichever node has the focus, aless writes the whole document, so you don't need to move the focus before you type `:w`. aless pretty-prints the JSON with two spaces a level. The JSON's keys keep the order they have in the source.

aless resolves a relative filename from the directory it started in. `:w` refuses a file that exists, so that a mistyped name doesn't overwrite an existing file. `:w!` writes over the file.

[Line mode](/how-to/display.html#data-mode-and-line-mode) shows the same JSON in the viewer, so you can read the JSON before you write it. In line mode, the tree keeps its folds. `m` switches line mode on and off.

## What the JSON holds

The file that `:w` writes holds the tree the viewer shows, not the text of the source:

- Comments aren't in the tree, so they aren't in the file.
- A number keeps its spelling where JSON spells it the same way, so `1.0` stays `1.0`. A spelling that JSON lacks gives the value it stands for, so `0x1F` becomes `31`. For more information, see [Formats](/reference/formats.html#numbers).
- A format whose tree has a shape of its own keeps that shape in the file. aless writes an XML file as element records, a Markdown file as its syntax tree, and a CSV file as an array of records. For more information, see [Formats](/reference/formats.html).

## Copy one value instead

`yy` copies the focused value as indented JSON. `yv` copies the same value as JSON on one line. When the focus is on a container, the value copied is the whole container, so `yy` on one path of an API description copies every operation under that path. For more information, see [Copy a value or its path](/how-to/copy.html).

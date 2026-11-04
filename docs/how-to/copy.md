---
title: Copy a value or its path
description: Copy the focused node's value, key, or path, on a desktop or over SSH, or print it on screen to read.
group: Reading a document
order: 5
---
Each copy command is `y` and a second key. The second key says what to copy: the node's value, its key, or its path. The copy commands come from jless. Move the focus to the node, and then type the two keys. The examples in the table come from the tutorial's file, [`bookshelf-openapi.yaml`](/examples/bookshelf-openapi.yaml). Here the focus is on the `operationId` of `listBooks`:

| Keys | Copies | Example |
|---|---|---|
| `yy` | the value, pretty-printed as JSON | `"listBooks"` |
| `yv` | the value as JSON on one line | `"listBooks"` |
| `ys` | a string's contents, without its quotes | `listBooks` |
| `yk` | the key | `operationId` |
| `yp` | the path, as jless writes it | `.paths["/api/shelf/{shelf_id}/book"].get.operationId` |
| `yb` | the path, every step in brackets | `["paths"]["/api/shelf/{shelf_id}/book"]["get"]["operationId"]` |
| `yq` | the path, as jq writes it | `.paths."/api/shelf/{shelf_id}/book".get.operationId` |

`yy` and `yv` differ on a container. `yy` copies an object as indented JSON, and `yv` copies the same object on one line.

`yq`'s path is the one jq takes, so it goes straight into a jq filter.

## Copy to the clipboard

aless copies to the system clipboard. The status line says so: `Copied path to the clipboard`.

Where aless can reach no clipboard, for example over SSH or in a container, it sends the text through the terminal instead. aless wraps the text in the OSC 52 escape sequence. The status line then names `the terminal clipboard (OSC 52)`. These terminals put that text on the clipboard of the machine they run on: xterm, kitty, WezTerm, iTerm2, Alacritty, foot, and Windows Terminal. Inside tmux, the sequence reaches the outer terminal when the tmux option `set-clipboard` lets it through.

## Print the text on screen

Each copy command has a print command to match. A print command starts with `p` instead of `y`. A print command shows the text on screen instead of copying it, to read or to select with the terminal's own mouse. The print commands are `pp`, `pv`, `ps`, `pk`, `pP`, `pb`, and `pq`. `pP` prints the path, because `pp` already prints the value.

The next example opens [`books.json`](/examples/books.json). With the focus on the first book, `pp` shows the book as JSON:

<!-- screen: aless books.json
keys: j p p
size: 80x12 -->
```screen
 pretty-printed value: j and k scroll, h and l pan, and any other key returns
{
  "title": "A Book of Examples",
  "author": "A. N. Author",
  "year": 1999,
  "tags": [
    "examples",
    "reference"
  ]
}
 books.json [0]                                           json · watching · 2:3
```

A long string shows whole this way. In the tree, by contrast, the edge of the screen cuts the string's row short.

Any key other than the scroll and pan keys returns you to the document.

In the [explorer](/how-to/explore.html), `yp` copies the focused entry's path on disk. The reference lists every copy and print command. For more information, see [Keys](/reference/keys.html#copy-and-print).

---
title: Copy a value or its path
description: Copy the focused node's value, its key or its path, on a desktop or over SSH, or print it on screen to read.
group: Reading a document
order: 5
---
The copy commands are jless's: `y` and a second key. Move the focus to the node, and type them. Here the focus is on the `operationId` of `listBooks`, in the [tutorial's file](/examples/bookshelf-openapi.yaml):

| Keys | Copies | Here |
|---|---|---|
| `yy` | the value, pretty-printed as JSON | `"listBooks"` |
| `yv` | the value as JSON on one line | `"listBooks"` |
| `ys` | a string's contents, without its quotes | `listBooks` |
| `yk` | the key | `operationId` |
| `yp` | the path, as jless writes it | `.paths["/api/shelf/{shelf_id}/book"].get.operationId` |
| `yb` | the path, every step in brackets | `["paths"]["/api/shelf/{shelf_id}/book"]["get"]["operationId"]` |
| `yq` | the path, as jq writes it | `.paths."/api/shelf/{shelf_id}/book".get.operationId` |

The two values differ on a container: `yy` copies an object as indented JSON, and `yv` copies it on one line. `yq`'s path is the one jq takes, so it goes straight into a jq filter.

## Where the copy goes

aless copies to the system clipboard, and the status line says so: `Copied path to the clipboard`. Where no clipboard can be reached, over SSH or in a container, it sends the text through the terminal instead, with the OSC 52 escape sequence, and the status line names `the terminal clipboard (OSC 52)`. xterm, kitty, WezTerm, iTerm2, Alacritty, foot and Windows Terminal put that text on the clipboard of the machine they run on. Inside tmux, the sequence reaches the outer terminal when tmux's `set-clipboard` option lets it through.

## Print it instead

Each copy command has a `p` twin that shows the text on screen instead of copying it, to read or to select with the terminal's own mouse: `pp`, `pv`, `ps`, `pk`, `pP` for the path (since `pp` is the value), `pb` and `pq`. With the focus on the first book in [`books.json`](/examples/books.json), `pp` shows it as JSON:

<!-- screen: aless books.json
keys: j p p
size: 80x12 -->
```screen
 pretty-printed value — j/k scroll, h/l pan, any other key returns
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

A long string shows whole this way, where its row in the tree is cut at the edge of the screen. Any key but those that scroll or pan goes back to the document.

In the [file explorer](/how-to/explore.html), `yp` copies the focused entry's path on disk. The [keys](/reference/keys.html#copy-and-print) list every copy and print command.

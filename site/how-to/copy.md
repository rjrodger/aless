---
title: Copy a value or its path
description: Copy the focused node's value, its key or its path from the viewer, on a desktop or over SSH.
order: 5
---
The copy commands are jless's: `y` and a second key. Move the focus to the node, and type them. With the focus on the `operationId` of `listBooks` in the [tutorial's file](/examples/bookshelf-openapi.yaml):

| Keys | Copies | Here |
|---|---|---|
| `yy` | the value, pretty-printed | `"listBooks"` |
| `yv` | the value on one line | `"listBooks"` |
| `ys` | a string's contents, without quotes | `listBooks` |
| `yk` | the key | `operationId` |
| `yp` | the path, as jless writes it | `.paths["/api/shelf/{shelf_id}/book"].get.operationId` |
| `yb` | the path, every step in brackets | `["paths"]["/api/shelf/{shelf_id}/book"]["get"]["operationId"]` |
| `yq` | the path, as jq writes it | `.paths."/api/shelf/{shelf_id}/book".get.operationId` |

The difference between a pretty and a one-line value shows on a container: `yy` copies an object as indented JSON, `yv` on one line. `yq`'s form is the one `--path` and jq both take, and the one every output of aless's command line prints.

## Where the copy goes

aless copies to the system clipboard, and the status line says so: `Copied path to the clipboard`. Where no clipboard can be reached, over SSH or in a container, it sends the text through the terminal instead, with the OSC 52 escape sequence, and the status line says `the terminal clipboard (OSC 52)`. xterm, kitty, WezTerm, iTerm2, Alacritty, foot and Windows Terminal put that text on your own machine's clipboard. Inside tmux, the sequence reaches the outer terminal when tmux's `set-clipboard` option lets it through.

## Print it instead

Each copy command has a `p` twin that prints the text on screen rather than copying it, for you to read, or to select with the terminal's own mouse selection: `pp`, `pv`, `ps`, `pk`, `pP` (the path, since `pp` is the value), `pb` and `pq`.

In the [file explorer](/how-to/explore.html), `yp` copies the focused entry's path on disk.

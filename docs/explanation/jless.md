---
title: aless and jless
description: What aless keeps from jless, what it adds, and where the two differ on purpose.
order: 5
---
aless follows the design of [jless](https://jless.io). jless is a terminal viewer for JSON and YAML. Paul Julius Martinez wrote it, and [its source is on GitHub](https://github.com/PaulJuliusMartinez/jless). jless set out how a large document reads in a terminal: a tree folded and unfolded with vim keys, a [data mode](/how-to/display.html#data-mode-and-line-mode) without the punctuation, a preview of what a folded node holds, and a path copied in two keystrokes. aless keeps that interface whole: the key map, the data mode and [line mode](/how-to/display.html#data-mode-and-line-mode), the folded previews, the regular-expression search, and the copy commands. Someone who knows the keys of jless knows the keys of aless.

aless is an independent implementation, not a fork. Its tree and its drawing are its own. It reads every format through the [tabnas](https://tabnas.dev) engine rather than through a JSON or YAML library. The notices of aless credit jless as the design aless follows. They also carry the licence of jless.

## What aless adds

jless reads JSON and YAML. aless reads every format the tabnas parsers read, and any format an ABNF grammar describes. It opens several files at once, each in a tab. A directory opens as an [explorer](/how-to/explore.html). aless watches every file it shows, so when a file changes, aless reads the file again and keeps your place. aless has a [source view](/how-to/display.html#data-mode-and-line-mode). aless can give the line and column of every node: the status bar shows the focused node's line and column in the file.

Each of those additions is on a key jless leaves free, so none of the keys of jless means something else in aless.

## Where aless and jless differ

| | aless | jless |
|---|---|---|
| Documents | Several are open at once, each in a tab. | One is open. |
| `q` | Closes the active tab, because several tabs can be open. aless quits only when that tab is the last. | Quits. |
| Search | Matches each node's own `"key": value` text, so a pattern cannot span two rows. | Reads the whole document as one line. |
| Numbering | Counts the rows on screen, in data mode as in line mode. The status bar gives the focused node's line and column in the file. | Counts the lines of the pretty-printed document. |
| `J` and `K` | Stop at the last sibling. | Track a depth asked for earlier. |
| Format | Comes from the file's extension, or from `--kind`. | `--json` and `--yaml` name the format. |

For more information about what the search reads, see [Search a document](/how-to/search.html). For more information about how aless numbers the rows, see [Choose how the viewer draws a document](/how-to/display.html).

If you read only JSON and YAML, one document at a time, jless is smaller and faster at what it does, because its parsers are built for those two formats alone. aless is for files of many formats, or for files that change while you read them.

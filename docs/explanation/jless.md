---
title: aless and jless
description: What aless takes from jless, the viewer it is modelled on, what it adds, and where the two behave differently, on purpose.
order: 5
---
[jless](https://jless.io), by Paul Julius Martinez, is the inspiration for aless. It is a terminal viewer for JSON and YAML ([its source](https://github.com/PaulJuliusMartinez/jless)), and it set out how a large document reads in a terminal: a tree folded and unfolded with vim's keys, a data mode without the punctuation, a preview of what a folded node holds, and a path copied in two keystrokes. aless keeps that interface whole: the key map, the data and line modes, the folded previews, the regular expression search and the copy commands. Someone who knows jless's keys knows aless's.

aless is an independent implementation, not a fork. Its tree, its drawing and its parsers are its own, and it reads every format through the [tabnas](https://tabnas.dev) engine rather than through a JSON or YAML library. Its notices credit jless, with jless's licence, as the design aless follows.

## What aless adds

aless reads every format the tabnas parsers read, and any format an ABNF grammar describes, where jless reads JSON and YAML. It opens several files at once, in tabs, and a directory as a file explorer. It watches every file it shows, and when one changes, reads it again and keeps your place. It has a source view, and knows the line and column of every node.

Each of those is on a key jless leaves free, so none of jless's keys means something else in aless.

## Where they differ

| aless | jless |
|---|---|
| Several files open in tabs, so `q` closes the active tab, and quits only when it was the last. | One document is open, and `q` quits. |
| The search matches each node's own `"key": value` text, so a pattern cannot span two rows ([Search a document](/how-to/search.html)). | The search reads the whole document as one line. |
| Row numbers count the rows on screen, in data mode as in line mode, and the status bar gives the focused node's line and column in the file ([Choose how the viewer draws a document](/how-to/display.html)). | Line numbers count the lines of the pretty-printed document. |
| `J` and `K` stop at the last sibling. | `J` and `K` track a depth asked for earlier. |
| A file's format comes from its extension, or from `--kind`. | `--json` and `--yaml` name the format. |

If you read only JSON and YAML, in one window, jless is smaller and faster at what it does, with parsers built for those two formats alone. aless is for when the files are of many formats, or change while you read them.

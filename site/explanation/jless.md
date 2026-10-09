---
title: aless and jless
description: What aless takes from jless, what it adds, and where the two behave differently, on purpose.
order: 6
---
[jless](https://jless.io) is a terminal viewer for JSON and YAML, by Paul Julius Martinez. aless reproduces its interface: the key map, data and line modes, collapsed previews, regular expression search and the copy commands. Someone who knows jless's keys knows aless's.

aless is an independent implementation, not a fork. Its tree, its rendering and its parsers are its own, and it reads every format through the [tabnas](https://tabnas.dev) engine rather than through a JSON or YAML library. jless is credited, with its licence, in aless's notices, as the design aless follows.

## What aless adds

- Every format the tabnas parsers read, and any format an ABNF grammar describes, where jless reads JSON and YAML.
- Tabs, one per file, and a file explorer.
- Watching: a changed file is reloaded with your place kept.
- A source view, and the line and column of every node.
- Panes that show a conversion or a program's output beside the document.
- The command line for scripts and agents, which prints JSON instead of opening the viewer.

Each extension is on a key jless leaves free, so none of jless's keys means something else in aless.

## Where they differ

- Several files open in tabs rather than one document, so `q` closes the focused tab and quits only when it was the last.
- Numbers are shown as their value (`1e+21`, `55`), not as the source spelled them, because the tree holds numbers as 64-bit floats ([One tree for every format](/explanation/one-tree.html)). The source view shows the spelling.
- Search matches each node's own `"key": value` text, so a pattern cannot span rows.
- `J` and `K` stop at the last sibling, rather than tracking a depth you asked for earlier.
- jless's `--json` and `--yaml` name the input's format. In aless the format comes from the extension or `--kind`, and `--json` asks for JSON output.

If you only read JSON and YAML, in one window, jless is smaller and faster at what it does, with parsers built for those two formats alone. aless is for when the files are of many formats, change while you read them, or are read by programs as often as by people.

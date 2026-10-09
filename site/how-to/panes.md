---
title: See a document beside its conversion
description: Open the output of --render or --alchemy in a pane next to the document, and keep it current as the document changes.
order: 11
---
`--panes out` opens a second pane beside the document, holding what `--render` or `--alchemy` would write for it, read back as a tree. It suits work on a conversion or a program, since the output follows the input as you edit it.

## Open the panes

Name the format to write, and open the panes:

```sh
aless --panes out --render yaml books.json
```

The document is on the left and its YAML on the right, both as trees. Without `--render` or `--alchemy` the output is the document as JSON. `--stacked` puts the panes one above the other, and `:arrange` switches between the two arrangements inside aless.

For a program, a third pane can show the program itself:

```sh
aless --panes out,program --alchemy table.alc --render yaml records.json
```

## Work in the panes

`C-w` moves the focus to the next pane, and the keys that move, fold, search and copy work on the focused pane. Its title is bold, and the status bar describes it. `s` switches the focused pane between its tree and its text, coloured by its format's grammar.

`q` closes an output or program pane; in the input pane it closes the tab, as it always does. `:pane out`, `:pane program` and `:pane close` open and close panes, `:vsplit` and `:split` open the output beside or below the input, and `:only` keeps the input alone.

## Keep the output current

The output is written again when the document reloads, when you read it as another format with `:format`, and when the program's file changes, which aless watches as it watches a document. `r` in the program pane reads the program again. The output keeps your place across each rebuild, as a reload does.

The panes are a viewer feature, so `--panes` needs a terminal: without one aless refuses with the status 2. The output pane keeps up to 16 MB, and shows a longer output as text, cut short, rather than as a tree.

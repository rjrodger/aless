---
title: Browse a directory
description: Open a directory as a tree in the viewer, find a file in it, and open the file in a tab.
order: 12
---
A directory opens in the viewer as a tree of its contents, which you move through, fold and search with the same keys as a document.

## Open a directory

Name it, or name nothing for the current directory:

```sh
aless ~/projects
aless
```

Directories are folds and files are leaves, each showing its size and the format its extension implies. A directory's contents are listed when you unfold it (`l` or `Space`), one level ahead, so a folded directory's preview already shows how many entries it has and their first names.

## Find and open a file

`/` searches the names listed so far, and `n` and `N` move between matches. `Enter` opens the focused file in a new tab, with its format taken from its extension; on a directory, `Enter` folds or unfolds it. `Tab` and `Shift-Tab` move between the tabs, and `q` closes the focused one.

## Move around the file system

| Keys | Do |
|---|---|
| `-` | go up: the parent directory becomes the root, with your folds and focus kept |
| `:cd DIR` | change the root, relative to the current one |
| `:explore DIR` | open another directory in a new tab |
| `:set hidden` | show dot-files (`nohidden` hides them, `hidden!` toggles); `--hidden` shows them from the start |
| `yp` | copy the focused entry's path on disk |

An explorer tab watches its directories as a file tab watches its file, so files added or removed show on the next tick, with your folds and focus kept. One explorer lists at most 2,000 directories.

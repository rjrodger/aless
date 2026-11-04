---
title: Browse a directory
description: Open a directory as a tree in the viewer, find a file in it, and open the file in a tab.
group: Files and formats
order: 10
---
A directory opens in the viewer as a tree of what it holds, which you move through, fold and search with the same keys as a document.

## Open a directory

Name it, or name nothing for the current directory:

<!-- test: skip it names directories of your own -->
```sh
aless ~/projects
aless
```

Directories are folds, and files are leaves, each with its size and the format its extension implies. Here is a directory of this site's [example files](/examples/books.json), opened from its parent as `aless --grammar hosts=bookshelf/hosts.abnf bookshelf`:

<!-- screen: aless --grammar hosts=bookshelf/hosts.abnf bookshelf
cwd: ..
size: 80x12 -->
```screen
▼ /home/user/bookshelf/
    books.json  359 B  json
    bookshelf-openapi.yaml  7.4 KB  yaml
    broken.json  74 B  json
    hosts  538 B  hosts
    hosts.abnf  358 B  abnf




 /home/user/bookshelf                          directory · 5 entries · watching
```

A directory's entries are listed when you unfold it (`l` or `Space`), one level ahead, so a folded directory's preview already shows how many entries it has and their first names. A file whose extension no format claims shows no format, and opens as plain text. The file named `hosts` is read as a format of its own because the grammar in `hosts.abnf` was named on the command line ([Read a format aless does not know](/how-to/grammar.html)).

## Find and open a file

`/` searches the entries listed so far, and reads each row whole, so `/json` finds every file that `json` names as its format. `n` and `N` move between the matches. `Enter` opens the focused file in a new tab, with its format taken from its extension, and folds or unfolds a directory. `Tab` and `Shift-Tab` move between the tabs, and `q` closes the active one ([Open several files in tabs](/how-to/tabs.html)).

## Move around the file system

| Keys | Do |
|---|---|
| `-` | go up: the parent directory becomes the root, with your folds and focus kept |
| `:cd DIR` | make DIR the root, relative to the current one |
| `:explore DIR` | open another directory in a new tab |
| `:set hidden` | show dot-files; `:set nohidden` hides them, `:set hidden!` toggles, and `--hidden` shows them from the start |
| `yp` | copy the focused entry's path on disk |

`m`, `%` and `s` mean nothing for a directory, and do nothing in the explorer.

An explorer tab watches its directories as a file's tab watches its file, so a file added or removed shows up within a second, with your folds and focus kept. One explorer lists at most 2,000 directories, and says so when it stops. The [keys](/reference/keys.html#the-explorer) list every explorer key.

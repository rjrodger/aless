---
title: Browse a directory
description: Open a directory as a tree in the viewer, find a file in the tree, and open that file in a tab.
group: Files and formats
order: 11
---
A directory opens in the viewer as the explorer, a tree of what the directory holds. You move through the tree, fold it, and search it with the same keys as a document.

## Open a directory

Name the directory, or name nothing for the current directory:

<!-- test: skip it opens directories on the reader's own disk -->
```sh
aless ~/projects
aless
```

In the tree, a directory is a fold and a file is a leaf. A file's row shows its size and the format its extension implies. This site's example files, such as [`books.json`](/examples/books.json), sit in one directory. From the parent of that directory, `aless --grammar hosts=bookshelf/hosts.abnf bookshelf` opens the directory of example files:

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

Unfold a directory with `l` or `Space`. aless then lists the directory's entries. The listing runs one level ahead, so a folded directory's preview already shows how many entries the directory has and the names of the first entries.

A file whose extension no format claims shows no format. It opens as plain text. aless reads the file named `hosts` as a format of its own, because the command line names the grammar in `hosts.abnf`. For more information, see [Read a format aless does not know](/how-to/grammar.html).

## Find and open a file

`/` searches the entries listed so far. The search reads each row whole, so `/json` finds every file whose format is `json`. `n` and `N` move between the matches.

`Enter` opens the focused file in a new tab. The new tab reads the file as the format its extension implies. With the focus on a directory, `Enter` folds or unfolds it instead.

`Tab` and `Shift+Tab` move between the tabs. `q` closes the active tab. For more information, see [Open several files in tabs](/how-to/tabs.html).

## Move around the file system

DIR stands for the path of a directory. The explorer takes these keys and commands:

| Keys | Effect |
|---|---|
| `-` | Go up: the parent directory becomes the root |
| `:cd DIR` | Make DIR the root, relative to the current root |
| `:explore DIR` | Open DIR in a new tab |
| `:set nohidden` | Hide dot-files, which the explorer shows by default |
| `:set hidden` | Show dot-files again |
| `:set hidden!` | Toggle dot-files |
| `yp` | Copy the focused entry's path on disk |

`-` keeps your folds and your focus.

`m`, `%`, and `s` do nothing in the explorer, because they mean nothing for a directory.

An explorer tab watches its directories, as a file's tab watches its file. When a file is added or removed, the tree shows the change within a second. Your folds and your focus stay as they were.

An explorer tab lists at most 2,000 directories. When the listing stops there, aless says so.

The reference lists every explorer key. For more information, see [Keys](/reference/keys.html#the-explorer).

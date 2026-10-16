---
title: Run aless on Linux, macOS and Windows
description: What the viewer needs from a terminal on each platform, what differs on Windows, and what to do over SSH, in a container, or without a clipboard.
group: Setting up
order: 3
---
aless runs on Linux, macOS and Windows, and its tests run on all three. The command line needs nothing of the terminal: without one, aless prints JSON. The viewer needs a terminal that understands VT escape sequences and a font that has its fold markers.

## The fold markers

The viewer draws a container's state with `▼ ▽ ▶ ▷`, which a font without those glyphs shows as boxes. `--ascii` draws `v` and `>` in their place:

```sh
aless --ascii data.json
```

## Windows

Use Windows Terminal, or another console that understands VT sequences. Piping into the viewer needs a console to read keys from, which a pipe takes away, so on Windows open a file by name rather than through a pipe; piping into the command line, as in `type data.json | aless --paths`, works as on any platform. `C-z`, which suspends aless, is Unix only. The installer puts `aless` in `~\.cargo\bin`, and the completions for PowerShell load from your profile ([Set up completions and the man page](/how-to/completions.html)).

## Over SSH and in a container

The copy commands use the system clipboard where there is one. Where there is none, over SSH or in a container, aless sends the text through the terminal with the OSC 52 escape sequence, and the status line says so; xterm, kitty, WezTerm, iTerm2, Alacritty, foot and Windows Terminal put that text on your own machine's clipboard. Inside tmux, the sequence reaches the outer terminal when tmux's `set-clipboard` option lets it through. The `p` twin of each copy command prints the text on screen instead ([Copy a value or its path](/how-to/copy.html)).

## Colour and the mouse

`--no-color`, or `NO_COLOR` set in the environment, draws without colour. `--no-mouse` leaves the mouse to the terminal, for its own selection and scrolling, where the viewer would otherwise take the wheel and clicks. A terminal that cannot draw the viewer at all, `TERM=dumb`, gets JSON on standard output instead, as a pipe does.

## Watching files

The viewer watches a file through the platform's own file watcher: inotify on Linux, FSEvents on macOS and ReadDirectoryChangesW on Windows, on the file's directory, so a save by renaming a new file over the old one is seen. A poll of each watched file's size and modification time every half second catches what a watcher misses, on a network share among other places. A directory's modification time moves only as often as its file system's clock ticks, 16 ms or so on NTFS and up to two seconds on FAT, so the explorer reads a changed directory again for two seconds after a change ([Browse a directory](/how-to/explore.html)).

The [reference](/reference/command-line.html#environment) lists the environment variables, and [Install aless](/how-to/install.html) the builds for each platform.

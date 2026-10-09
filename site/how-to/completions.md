---
title: Set up completions and the man page
description: Have your shell complete aless's options and their values, and man show its manual page, however you installed aless.
order: 2
---
aless carries its own manual page and completions for bash, zsh, fish and PowerShell. Homebrew installs them for you; with any other install, aless prints them.

## After a Homebrew install

The formula puts each file where Homebrew keeps them, so all that is left is for your shell to look there. Homebrew's [shell completion guide](https://docs.brew.sh/Shell-Completion) has the steps for each shell: for zsh, `eval "$(brew shellenv)"` has to run before `compinit`. `man aless` works as soon as the formula is installed.

## Load the completions as the shell starts

Add one line to your shell's startup file:

```sh
eval "$(aless --generate complete-bash)"     # ~/.bashrc
eval "$(aless --generate complete-zsh)"      # ~/.zshrc, after compinit
aless --generate complete-fish | source      # ~/.config/fish/config.fish
```

In PowerShell, add them to your profile once:

```powershell
aless --generate complete-powershell | Add-Content $PROFILE
```

The completions know every option and the values each one takes: the formats `--kind` reads and `--render` writes, the modes, and a file where an option wants one.

## Or save them where the shell looks

Write each file once instead:

```sh
aless --generate complete-bash > ~/.local/share/bash-completion/completions/aless
aless --generate complete-zsh > ~/.zfunc/_aless          # a directory on $fpath
aless --generate complete-fish > ~/.config/fish/completions/aless.fish
```

A saved file is read without running aless as the shell starts. Write it again when you upgrade aless, since a release can add options.

## The man page

Save it where `man` looks for your own pages:

```sh
mkdir -p ~/.local/share/man/man1
aless --generate man > ~/.local/share/man/man1/aless.1
man aless
```

If your `man` does not search `~/.local/share/man`, add it to `MANPATH`. The manual page holds the same reference as `aless --help`, and as this site's [command line reference](/reference/command-line.html).

---
title: Set up completions and the man page
description: Have your shell complete the options of aless and their values, and man show the aless man page, however you installed aless.
group: Setting up
order: 2
---
aless carries its own man page and completions for bash, zsh, fish, and PowerShell. Homebrew installs both for you. With any other install, aless prints them with its `--generate` option.

## Finish a Homebrew install

The formula puts the completions and the man page where Homebrew keeps them, so what's left is for your shell to look there. The Homebrew [shell completion guide](https://docs.brew.sh/Shell-Completion) has the steps for each shell. For zsh, `eval "$(brew shellenv)"` must run before `compinit`. The man page needs no step of your own: `man aless` works as soon as you have installed the formula.

## Load the completions as the shell starts

Add one line to your shell's startup file:

<!-- test: skip it changes your shell's startup file; tests/binary.rs checks what --generate prints -->
```sh
eval "$(aless --generate complete-bash)"     # ~/.bashrc
eval "$(aless --generate complete-zsh)"      # ~/.zshrc, after compinit
aless --generate complete-fish | source      # ~/.config/fish/config.fish
```

In PowerShell, add the completions to your profile once:

<!-- test: skip it changes your PowerShell profile -->
```powershell
aless --generate complete-powershell | Add-Content $PROFILE
```

The completions offer every option. They offer each option's values too: the formats `--kind` reads, the two modes, and a file where an option takes one.

## Save the completions where the shell looks

Instead of adding a line to your startup file, save each file once:

<!-- test: skip it writes into your home directory; tests/binary.rs checks what --generate prints -->
```sh
aless --generate complete-bash > ~/.local/share/bash-completion/completions/aless
aless --generate complete-zsh > ~/.zfunc/_aless          # a directory on $fpath
aless --generate complete-fish > ~/.config/fish/completions/aless.fish
```

The shell then reads the saved file instead of running aless as it starts. Save the files again when you upgrade aless, since a release can add options.

## Save the man page

Save the man page where `man` looks for your own pages:

<!-- test: skip it writes into your home directory; tests/binary.rs checks the man page --generate prints -->
```sh
mkdir -p ~/.local/share/man/man1
aless --generate man > ~/.local/share/man/man1/aless.1
man aless
```

If your `man` doesn't search `~/.local/share/man`, add that directory to `MANPATH`. The man page holds the same reference as `aless --help` and this site's [Command line](/reference/command-line.html) page.

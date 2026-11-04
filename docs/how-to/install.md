---
title: Install aless
description: Install aless on Linux, macOS or Windows, with its installer, Homebrew or Cargo, and check what you downloaded.
group: Setting up
order: 1
---
aless is one binary, with nothing to install beside it. Every release is on [GitHub Releases](https://github.com/rjrodger/aless/releases), built for Linux, macOS and Windows, each on x86_64 and on aarch64.

## With the installer

On Linux or macOS:

<!-- test: skip it installs aless from a release -->
```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/rjrodger/aless/releases/latest/download/aless-installer.sh | sh
```

On Windows, in PowerShell:

<!-- test: skip it installs aless from a release -->
```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/rjrodger/aless/releases/latest/download/aless-installer.ps1 | iex"
```

Both put `aless` in the `bin` directory of `CARGO_HOME`, which is `~/.cargo/bin` unless you have set it. Neither installs anything that updates aless later. On Linux, the shell installer picks the build for your C library: the one built against glibc 2.35, or the static one, which runs anywhere.

## With Homebrew

On macOS or Linux:

<!-- test: skip it installs aless with Homebrew -->
```sh
brew install rjrodger/tap/aless
```

The formula installs the man page and the completions for bash, zsh, fish and PowerShell as well. [Set up completions and the man page](/how-to/completions.html) says what your shell needs to find them.

## With Cargo

With Rust 1.88 or newer:

<!-- test: skip it installs aless from crates.io -->
```sh
cargo install --locked aless
```

`--locked` builds with the versions of the dependencies aless was tested with. `cargo binstall aless` fetches the prebuilt binary instead of compiling it. To build the newest commit rather than a release:

<!-- test: skip it installs aless from the repository -->
```sh
cargo install --locked --git https://github.com/rjrodger/aless aless
```

## Check what you downloaded

Each archive on a release has a `.sha256` file beside it, and a build provenance attestation, which the GitHub CLI checks. For the x86_64 Linux build:

<!-- test: skip it checks an archive downloaded from a release -->
```sh
sha256sum -c aless-x86_64-unknown-linux-gnu.tar.xz.sha256
gh attestation verify aless-x86_64-unknown-linux-gnu.tar.xz --repo rjrodger/aless
```

An archive holds the binary, the man page and the completions, with the licence and the changelog.

## Check that it runs

Ask for its version:

```console
$ aless --version
aless {{version}}
```

[Run aless on Linux, macOS and Windows](/how-to/platforms.html) says what each platform needs from a terminal, and the [tutorial](/tutorials/viewer.html) starts from here.

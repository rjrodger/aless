---
title: Install aless
description: Install aless on Linux, macOS or Windows, from a release, with Homebrew or with Cargo, and check what you downloaded.
order: 1
---
aless is one binary with nothing else to install beside it. Every release is on [GitHub Releases](https://github.com/rjrodger/aless/releases), built for Linux, macOS and Windows, each on x86_64 and on aarch64.

## With the installer

On Linux or macOS:

```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/rjrodger/aless/releases/latest/download/aless-installer.sh | sh
```

On Windows, in PowerShell:

```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/rjrodger/aless/releases/latest/download/aless-installer.ps1 | iex"
```

Both put `aless` in the `bin` directory of `CARGO_HOME`, which is `~/.cargo/bin` unless you have set it, where `cargo install` would put it. Neither installs anything that updates aless later. On Linux the shell installer picks the build for your C library, or the static build, which runs anywhere.

## With Homebrew

On macOS or Linux:

```sh
brew install rjrodger/tap/aless
```

The formula installs the man page and the completions for bash, zsh, fish and PowerShell as well. [Set up completions and the man page](/how-to/completions.html) says what your shell needs to read them.

## With Cargo

With a Rust toolchain, version 1.88 or newer:

```sh
cargo install --locked aless
```

`--locked` builds with the dependency versions aless was tested with. `cargo binstall aless` fetches the prebuilt binary instead of compiling it.

To build the newest commit rather than a release:

```sh
cargo install --locked --git https://github.com/rjrodger/aless aless
```

## Check what you downloaded

Each archive on a release has a `.sha256` file beside it, and a build provenance attestation, which the GitHub CLI checks. For the x86_64 Linux build:

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

The [tutorials](/tutorials/index.html) start from here.

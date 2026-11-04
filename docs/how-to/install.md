---
title: Install aless
description: Install aless on Linux, macOS, or Windows with its installer, Homebrew, or Cargo, and check what you downloaded.
group: Setting up
order: 1
---
aless is one binary, with nothing to install beside it. Every release is on [GitHub Releases](https://github.com/rjrodger/aless/releases). Each release has builds for Linux, macOS, and Windows, each on x86_64 and on aarch64.

## Run the installer

On Linux or macOS, run the shell installer:

<!-- test: skip it installs aless from a release -->
```sh
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/rjrodger/aless/releases/latest/download/aless-installer.sh | sh
```

On Windows, run the installer in PowerShell:

<!-- test: skip it installs aless from a release -->
```powershell
powershell -ExecutionPolicy Bypass -c "irm https://github.com/rjrodger/aless/releases/latest/download/aless-installer.ps1 | iex"
```

Both installers put `aless` in the `bin` directory of `CARGO_HOME`. Unless you've set `CARGO_HOME`, the `bin` directory is `~/.cargo/bin`. Neither installer adds anything that updates aless later.

On Linux, the shell installer picks one of two builds for your C library. One is built against glibc 2.35. The other is static, so it runs anywhere.

## Use Homebrew

On macOS or Linux, install the formula:

<!-- test: skip it installs aless with Homebrew -->
```sh
brew install rjrodger/tap/aless
```

The formula also installs the man page and the completions for bash, zsh, fish, and PowerShell. For more information about what your shell needs to find the completions and the man page, see [Set up completions and the man page](/how-to/completions.html).

## Use Cargo

You need Rust 1.88 or newer. To build and install aless, run:

<!-- test: skip it installs aless from crates.io -->
```sh
cargo install --locked aless
```

`--locked` builds with the versions of the dependencies that aless was tested with.

`cargo binstall aless` fetches the prebuilt binary instead of compiling aless.

To build the newest commit rather than a release, run:

<!-- test: skip it installs aless from the repository -->
```sh
cargo install --locked --git https://github.com/rjrodger/aless aless
```

## Check what you downloaded

Each archive on a release has a `.sha256` file beside it. Each archive also has a build-provenance attestation. The GitHub CLI checks that attestation. For the x86_64 Linux build, check the archive against its `.sha256` file and its attestation:

<!-- test: skip it checks an archive downloaded from a release -->
```sh
sha256sum -c aless-x86_64-unknown-linux-gnu.tar.xz.sha256
gh attestation verify aless-x86_64-unknown-linux-gnu.tar.xz --repo rjrodger/aless
```

An archive holds the binary, the man page, the completions, the licence, and the changelog.

## Check that aless runs

Ask aless for its version:

```console
$ aless --version
aless {{version}}
```

For more information about what each platform needs from a terminal, see [Run aless on Linux, macOS, and Windows](/how-to/platforms.html). The tutorial [Look around a file in the viewer](/tutorials/viewer.html) starts from here.

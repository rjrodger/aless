# Packaging aless

This page is for anyone packaging aless for a package manager or a
distribution, and for the owner submitting it to one. For more
information about making a release, see [Releasing aless](RELEASING.md).

## Building

A packager needs these facts:

- The build needs Rust 1.88 or newer. `Cargo.toml` records that version
  as `rust-version`. CI checks the build against that version.
- The build command is `cargo build --release --locked`. `Cargo.lock`
  pins every dependency. Every dependency comes from crates.io, so there
  are no git or path dependencies. For an offline build, run
  `cargo fetch --locked` first, and then `cargo build --release --frozen`.
- The build needs no C compiler and no system libraries, because aless
  and everything it depends on are Rust. The clipboard speaks X11 and
  Wayland itself, so it needs neither libxcb nor libwayland. A static
  `*-unknown-linux-musl` build needs nothing extra.
- The build has one feature, `clipboard`, and the feature is on by
  default. With it, the copy commands copy to the system clipboard.
  The copy goes through arboard. `--no-default-features` drops the
  feature, and the copy commands then copy through the terminal instead.
  That copy uses OSC 52.
- A package installs these files:
  - The binary is `target/release/aless`, or `aless.exe` on Windows.
  - The licence files are `LICENSE` and `THIRD_PARTY_NOTICES.md`.
    `LICENSE` is the MIT licence. `THIRD_PARTY_NOTICES.md` is the MIT
    notice from jless.
  - The documentation is `README.md`.
  - The man page and the shells' completions go where each system
    keeps them:

    | File | Installed as, on Linux |
    |---|---|
    | `man/aless.1` | `share/man/man1/aless.1` |
    | `completions/aless.bash` | `share/bash-completion/completions/aless` |
    | `completions/_aless` | `share/zsh/site-functions/_aless` |
    | `completions/aless.fish` | `share/fish/vendor_completions.d/aless.fish` |
    | `completions/_aless.ps1` | Where PowerShell keeps it. A user adds it to `$PROFILE`. |

  The source and the crate carry the committed man page and
  completions. The built binary writes each of them too, with
  `aless --generate man`, `complete-bash`, `complete-zsh`,
  `complete-fish`, or `complete-powershell`. Those are the names ripgrep
  uses, so a Homebrew formula takes them as the ripgrep formula does:

  ```ruby
  generate_completions_from_executable(bin/"aless", "--generate", shell_parameter_format: "complete-")
  (man1/"aless.1").write Utils.safe_popen_read(bin/"aless", "--generate", "man")
  ```
- The name is free. On 8 October 2026, a search found no package called
  `aless` in any repository Repology tracks, nor in Homebrew, Arch
  Linux, the Arch User Repository (AUR), or crates.io.

## Testing

A package build can run these checks:

- The whole test suite. No test needs the network, so a sandboxed build
  runs it all:

  ```bash
  cargo test --locked
  ```

- A smoke test of the built binary in a pseudo-terminal. On Unix,
  `python3 admin/scripts/pty-smoke.py target/release/aless` runs that
  smoke test.

## Sources and binaries

Every release is a GitHub Release named `vX.Y.Z`, where `X.Y.Z` is the
version. Each release carries:

- The source, as `source.tar.gz`. It is the same as the GitHub
  archive `archive/refs/tags/vX.Y.Z.tar.gz`.
- Prebuilt archives for Linux, macOS, and Windows, each for x86_64 and
  aarch64. The Linux archives come in two kinds: one for glibc 2.35 or
  newer, and one static, with musl.
- A `.sha256` for each file, and `sha256.sum` for all of them.
- A CycloneDX software bill of materials (SBOM).
- Build-provenance attestations.
  `gh attestation verify FILE --repo rjrodger/aless` checks one, where
  `FILE` is a file from the release.

The crate on crates.io holds the same source, without the tests.

## Channels

These are the channels, their install commands, how aless gets to each,
and who keeps it current there:

| Channel | Install command | How aless gets there | Who keeps it current |
|---|---|---|---|
| GitHub Releases, with the shell and PowerShell installers | | `release.yml` | The release workflow |
| Homebrew tap | `brew install rjrodger/tap/aless` | `release.yml` pushes the formula from dist through `publish-homebrew.yml`. | The release workflow |
| crates.io | `cargo install --locked aless` | The first version by hand, and then `publish-crates.yml` | The release workflow |
| cargo-binstall | `cargo binstall aless` | cargo-binstall finds the release archives by their names, so it needs no metadata. | Nobody |
| homebrew-core | `brew install aless` | A pull request, once aless meets the Homebrew [acceptance policy](https://docs.brew.sh/Package-Acceptance-Policy) | Homebrew |
| MacPorts | `sudo port install aless` | A Portfile with the `cargo` portgroup, sent as a pull request to macports-ports. The Portfile's crate list comes from `cargo2port`. | A MacPorts maintainer |
| Arch Linux: AUR | | `aless`, built from source, or `aless-bin`, from the release archives, or both. The official repositories are the Arch packagers' choice. | Whoever adopts it |
| Void Linux | `sudo xbps-install aless` | A void-packages template with `build_style=cargo` | Void maintainers |
| NetBSD pkgsrc | `pkgin install aless` | pkgsrc-wip first, with `cargo-depends.mk` | pkgsrc developers |
| FreeBSD | `pkg install aless` | A port with `USES=cargo`, sent through Bugzilla. `make cargo-crates` lists the crates. | A FreeBSD committer |
| nixpkgs | | `pkgs/by-name/al/aless/package.nix`, built with `rustPlatform.buildRustPackage` | nixpkgs maintainers |
| winget | | A first manifest from `komac` or `wingetcreate`, as a portable zip, sent as a pull request to microsoft/winget-pkgs. `release.yml` can automate later ones. | The release workflow, once added |
| Scoop | | A separate bucket for aless, with the Windows zip. The main bucket asks for more popularity. | The release workflow, once added |
| Debian, Ubuntu, and Fedora | | Each packages every crate separately, so these channels follow the distributions' Rust teams. The tabnas crates are among those crates. | Their Rust teams |

The tap's formula is the one dist writes. That formula installs the
prebuilt binary. The man page and the bash, zsh, fish, and PowerShell
completions go where the formulas in Homebrew itself put theirs. The
formula also has a `brew test`.

For homebrew-core, the Homebrew acceptance policy asks for the following:

- A formula built from source, with `depends_on "rust" => :build` and
  `cargo install *std_cargo_args`. The man page and the completions come
  from `--generate`, as in [Building](#building).
- A description of at most 80 characters that does not start with an
  article. The description in `Cargo.toml` meets that rule.
- Enough GitHub stars, forks, or watchers.

## Starting points

These starting points are for the channels the owner submits to, once a
release exists. Each is a first draft, so check it with the channel's
own tools before you submit it. Each starting point lives in that
channel's repository, not in this one.

### AUR, from source

The package `aless` builds from the tag's source archive, so that
`check()` runs the tests that a sandbox can run:

```bash
pkgname=aless
pkgver=0.6.0
pkgrel=1
pkgdesc='Terminal viewer for JSON, YAML, TOML, CSV, XML and more, as one tree'
arch=('x86_64' 'aarch64')
url='https://aless.tabnas.dev'
license=('MIT')
depends=('gcc-libs' 'glibc')
makedepends=('cargo')
source=("$pkgname-$pkgver.tar.gz::https://github.com/rjrodger/aless/archive/refs/tags/v$pkgver.tar.gz")
sha256sums=('SKIP') # updpkgsums fills it

prepare() {
  cd "$pkgname-$pkgver"
  export RUSTUP_TOOLCHAIN=stable
  cargo fetch --locked --target "$(rustc -vV | sed -n 's/host: //p')"
}

build() {
  cd "$pkgname-$pkgver"
  export RUSTUP_TOOLCHAIN=stable CARGO_TARGET_DIR=target
  cargo build --frozen --release
}

check() {
  cd "$pkgname-$pkgver"
  export RUSTUP_TOOLCHAIN=stable CARGO_TARGET_DIR=target
  cargo test --frozen
}

package() {
  cd "$pkgname-$pkgver"
  install -Dm755 target/release/aless -t "$pkgdir/usr/bin/"
  install -Dm644 man/aless.1 -t "$pkgdir/usr/share/man/man1/"
  install -Dm644 completions/aless.bash "$pkgdir/usr/share/bash-completion/completions/aless"
  install -Dm644 completions/_aless -t "$pkgdir/usr/share/zsh/site-functions/"
  install -Dm644 completions/aless.fish -t "$pkgdir/usr/share/fish/vendor_completions.d/"
  install -Dm644 README.md -t "$pkgdir/usr/share/doc/$pkgname/"
  install -Dm644 LICENSE THIRD_PARTY_NOTICES.md -t "$pkgdir/usr/share/licenses/$pkgname/"
}
```

### AUR, prebuilt

The package `aless-bin` has the same `package()`, except for the
binary's path. That path is `aless` at the top of the archive's
directory. `aless-bin` has these lines of its own:

```bash
pkgname=aless-bin
provides=('aless')
conflicts=('aless')
source_x86_64=("https://github.com/rjrodger/aless/releases/download/v$pkgver/aless-x86_64-unknown-linux-gnu.tar.xz")
source_aarch64=("https://github.com/rjrodger/aless/releases/download/v$pkgver/aless-aarch64-unknown-linux-gnu.tar.xz")
sha256sums_x86_64=('SKIP') # updpkgsums fills them
sha256sums_aarch64=('SKIP')
# and in package(): cd "aless-$CARCH-unknown-linux-gnu"
```

Before the first push to the AUR, run these commands on each package:

1. Run `updpkgsums`.
2. Run `makepkg -s`.
3. Run `namcap`.
4. Run `makepkg --printsrcinfo > .SRCINFO`.

### Scoop

The manifest goes in a bucket of the owner's own, `rjrodger/scoop-bucket`.
The bucket starts from the [bucket
template](https://github.com/ScoopInstaller/BucketTemplate) on GitHub.
The template's workflow keeps the manifest current through `checkver`
and `autoupdate`. The Windows zips hold `aless.exe` at their top, so
the manifest's `bin` names it without a directory:

```json
{
    "version": "0.6.0",
    "description": "Terminal viewer for JSON, YAML, TOML, CSV, XML and more, as one tree",
    "homepage": "https://aless.tabnas.dev",
    "license": "MIT",
    "architecture": {
        "64bit": {
            "url": "https://github.com/rjrodger/aless/releases/download/v0.6.0/aless-x86_64-pc-windows-msvc.zip",
            "hash": "the first word of aless-x86_64-pc-windows-msvc.zip.sha256"
        },
        "arm64": {
            "url": "https://github.com/rjrodger/aless/releases/download/v0.6.0/aless-aarch64-pc-windows-msvc.zip",
            "hash": "the first word of aless-aarch64-pc-windows-msvc.zip.sha256"
        }
    },
    "bin": "aless.exe",
    "checkver": "github",
    "autoupdate": {
        "architecture": {
            "64bit": {
                "url": "https://github.com/rjrodger/aless/releases/download/v$version/aless-x86_64-pc-windows-msvc.zip"
            },
            "arm64": {
                "url": "https://github.com/rjrodger/aless/releases/download/v$version/aless-aarch64-pc-windows-msvc.zip"
            }
        },
        "hash": {
            "url": "$url.sha256"
        }
    }
}
```

Before the push, try the manifest with `scoop install ./aless.json`.

### winget

`komac new` or `wingetcreate new` makes the first manifest from the
two Windows zip URLs. [komac](https://github.com/russellbanks/Komac)
is on GitHub. The manifest describes a portable installer, with
`InstallerType: zip`, `NestedInstallerType: portable`, and `aless.exe`.
Its identifier is `rjrodger.aless`. The manifest goes to
microsoft/winget-pkgs as a pull request.

To submit a later version, run
`komac update rjrodger.aless --version X.Y.Z --urls URL URL --submit`,
where `X.Y.Z` is the version and each `URL` is one of the two Windows
zip URLs. A job in `publish-jobs` can run that command with a token of
the owner's. For more information about why that job goes in
`publish-jobs`, and not on a `release` event, see
[Releasing aless](RELEASING.md).

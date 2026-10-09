# Packaging aless

For anyone packaging aless for a package manager or a distribution, and
for the owner submitting it to one. How a release is made is in
[RELEASING.md](RELEASING.md).

## Building

- **Rust 1.88 or newer** (`rust-version` in `Cargo.toml`, checked in CI).
- **`cargo build --release --locked`.** `Cargo.lock` pins every
  dependency, and every one comes from crates.io: there are no git or path
  dependencies. For an offline build, run `cargo fetch --locked` first,
  then `cargo build --release --frozen`.
- **No C compiler and no system libraries.** aless and everything it
  depends on are Rust. The clipboard speaks X11 and Wayland itself, with
  neither libxcb nor libwayland. A static `*-unknown-linux-musl` build
  needs nothing extra.
- **One feature, `clipboard`, on by default.** It gives the yank commands
  the system clipboard (through arboard). `--no-default-features` drops
  it, and yank then copies through the terminal (OSC 52) instead.
- **What to install:**
  - the binary, `target/release/aless` (`aless.exe` on Windows);
  - `LICENSE` (MIT) and `THIRD_PARTY_NOTICES.md` (jless's MIT notice)
    as its licence files;
  - `README.md` as its documentation;
  - the man page and the shells' completions, where each system keeps
    them:

    | File | Installed as, on Linux |
    |---|---|
    | `man/aless.1` | `share/man/man1/aless.1` |
    | `completions/aless.bash` | `share/bash-completion/completions/aless` |
    | `completions/_aless` | `share/zsh/site-functions/_aless` |
    | `completions/aless.fish` | `share/fish/vendor_completions.d/aless.fish` |
    | `completions/_aless.ps1` | PowerShell's: a user adds it to `$PROFILE` |

  The source and the crate carry those files as they are committed. The
  built binary writes each of them too, with `aless --generate man`,
  `complete-bash`, `complete-zsh`, `complete-fish` or
  `complete-powershell`, the names ripgrep uses, so a Homebrew formula
  takes them as it takes ripgrep's:

  ```ruby
  generate_completions_from_executable(bin/"aless", "--generate", shell_parameter_format: "complete-")
  (man1/"aless.1").write Utils.safe_popen_read(bin/"aless", "--generate", "man")
  ```

  `aless --generate skill` writes the Agent Skill
  (`skills/aless/SKILL.md`), for a package that offers one.
- **The name.** On 2026-10-08 no package called `aless` was found in any
  repository Repology tracks, nor in Homebrew, Arch Linux, the AUR or
  crates.io.

## Testing

- **One test target needs the network.** `tests/yaml_render.rs` reads
  tabnas/yaml's own fixtures from a checkout of that repository at the
  locked version's tag (`TABNAS_YAML_DIR`; `scripts/yaml-fixtures.sh`
  clones it). A sandboxed build can run everything else:

  ```bash
  cargo test --locked --lib --bins --test agent --test app_flow --test formats --test render_memory
  ```

- **The viewer in a terminal.** On Unix,
  `python3 scripts/pty-smoke.py target/release/aless` drives the built
  binary in a pseudo-terminal.

## Sources and binaries

Every release is a GitHub Release named `v<version>`. It carries:
- the source as `source.tar.gz`, the same as GitHub's
  `archive/refs/tags/v<version>.tar.gz`;
- prebuilt archives for Linux (x86_64 and aarch64, glibc 2.35 or newer,
  or static musl), macOS (x86_64 and aarch64) and Windows (x86_64 and
  aarch64);
- a `.sha256` for each file, and `sha256.sum` for all of them;
- a CycloneDX SBOM;
- build-provenance attestations, checked with
  `gh attestation verify FILE --repo rjrodger/aless`.

The crate on crates.io holds the same source, without the tests.

## Channels

| Channel | How aless gets there | Who keeps it current |
|---|---|---|
| GitHub Releases, shell and PowerShell installers | `release.yml` | the release workflow |
| Homebrew tap: `brew install rjrodger/tap/aless` | `release.yml` pushes the formula through `publish-homebrew.yml`: dist's, which installs the prebuilt binary, with the man page and the bash, zsh, fish and PowerShell completions installed where Homebrew's own formulas put theirs, and a `brew test` | the release workflow |
| crates.io: `cargo install --locked aless` | the first version by hand, then `publish-crates.yml` | the release workflow |
| cargo-binstall: `cargo binstall aless` | finds the release archives by their names; no metadata needed | nobody |
| homebrew-core: `brew install aless` | a pull request once aless meets Homebrew's [acceptance policy](https://docs.brew.sh/Package-Acceptance-Policy): built from source with `depends_on "rust" => :build` and `cargo install *std_cargo_args`, the man page and the completions from `--generate` (above); a description of at most 80 characters that does not start with an article (`Cargo.toml`'s is one); and enough GitHub stars, forks or watchers | Homebrew |
| MacPorts: `sudo port install aless` | a Portfile using the `cargo` portgroup (its crate list from `cargo2port`), sent as a pull request to macports-ports | a MacPorts maintainer |
| Arch Linux: AUR | `aless`, built from source, and/or `aless-bin`, from the release archives; the official repositories are the Arch packagers' choice | whoever adopts it |
| Void Linux: `sudo xbps-install aless` | a void-packages template with `build_style=cargo` | Void maintainers |
| NetBSD pkgsrc: `pkgin install aless` | pkgsrc-wip first, using `cargo-depends.mk` | pkgsrc developers |
| FreeBSD: `pkg install aless` | a port with `USES=cargo` (`make cargo-crates` lists the crates), sent through Bugzilla | a FreeBSD committer |
| nixpkgs | `pkgs/by-name/al/aless/package.nix`, built with `rustPlatform.buildRustPackage` | nixpkgs maintainers |
| winget | a first manifest from `komac` or `wingetcreate` (portable zip), as a pull request to microsoft/winget-pkgs; later ones can be automated from `release.yml` | the release workflow, once added |
| Scoop | a bucket of our own with the Windows zip; the main bucket asks for more popularity | the release workflow, once added |
| Debian, Ubuntu, Fedora | each packages every crate separately, the 21 tabnas crates among them, so these follow the distributions' Rust teams | their Rust teams |

## Starting points

For the channels the owner submits to, once a release exists. Each is a
first draft to check with the channel's own tools before it goes up, and
lives in that channel's repository, not this one.

**AUR, from source** (`aless`), from the tag's source archive, so that
`check()` runs the tests a sandbox can:

```bash
pkgname=aless
pkgver=0.1.1
pkgrel=1
pkgdesc='Terminal viewer and JSON CLI for JSON, YAML, TOML, CSV, XML and more'
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
  cargo test --frozen --lib --bins --test agent --test app_flow --test formats --test render_memory
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

**AUR, prebuilt** (`aless-bin`): the same `package()`, but for the
binary's path, which is `aless` at the top of the archive's directory,
with:

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

Run `updpkgsums`, `makepkg -s` and `namcap` on each, then
`makepkg --printsrcinfo > .SRCINFO`, before the first push to the AUR.

**Scoop**, in a bucket of the owner's own (`rjrodger/scoop-bucket`, made
from the [bucket template](https://github.com/ScoopInstaller/BucketTemplate),
whose workflow keeps the manifest current through `checkver` and
`autoupdate`). The Windows zips hold `aless.exe` at their top:

```json
{
    "version": "0.1.1",
    "description": "Terminal viewer and JSON CLI for JSON, YAML, TOML, CSV, XML and more",
    "homepage": "https://aless.tabnas.dev",
    "license": "MIT",
    "architecture": {
        "64bit": {
            "url": "https://github.com/rjrodger/aless/releases/download/v0.1.1/aless-x86_64-pc-windows-msvc.zip",
            "hash": "the first word of aless-x86_64-pc-windows-msvc.zip.sha256"
        },
        "arm64": {
            "url": "https://github.com/rjrodger/aless/releases/download/v0.1.1/aless-aarch64-pc-windows-msvc.zip",
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

`scoop install ./aless.json` tries it before the push.

**winget**: the first manifest is made by
[komac](https://github.com/russellbanks/Komac) (`komac new`) or
`wingetcreate new` from the two Windows zip URLs, as a portable
installer (`InstallerType: zip`, `NestedInstallerType: portable`,
`aless.exe`), with the identifier `rjrodger.aless`, and goes to
microsoft/winget-pkgs as a pull request. Later versions are
`komac update rjrodger.aless --version X.Y.Z --urls URL URL --submit`,
which a job in `publish-jobs` can run with a token of the owner's
(RELEASING.md says why there, and not on a `release` event).

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
  - `LICENSE` (MIT) and `THIRD_PARTY_NOTICES.md` (the MIT notices of
    jless and of the AQL aless) as its licence files;
  - `README.md` as its documentation.

  There is no man page or shell completion yet.
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
| Homebrew tap: `brew install rjrodger/tap/aless` | `release.yml` pushes the formula | the release workflow |
| crates.io: `cargo install --locked aless` | the first version by hand, then `publish-crates.yml` | the release workflow |
| cargo-binstall: `cargo binstall aless` | finds the release archives by their names; no metadata needed | nobody |
| homebrew-core: `brew install aless` | a pull request once aless meets Homebrew's [acceptance policy](https://docs.brew.sh/Package-Acceptance-Policy): built from source with `depends_on "rust" => :build` and `cargo install *std_cargo_args`; a description of at most 80 characters that does not start with an article; and enough GitHub stars, forks or watchers | Homebrew |
| MacPorts: `sudo port install aless` | a Portfile using the `cargo` portgroup (its crate list from `cargo2port`), sent as a pull request to macports-ports | a MacPorts maintainer |
| Arch Linux: AUR | `aless`, built from source, and/or `aless-bin`, from the release archives; the official repositories are the Arch packagers' choice | whoever adopts it |
| Void Linux: `sudo xbps-install aless` | a void-packages template with `build_style=cargo` | Void maintainers |
| NetBSD pkgsrc: `pkgin install aless` | pkgsrc-wip first, using `cargo-depends.mk` | pkgsrc developers |
| FreeBSD: `pkg install aless` | a port with `USES=cargo` (`make cargo-crates` lists the crates), sent through Bugzilla | a FreeBSD committer |
| nixpkgs | `pkgs/by-name/al/aless/package.nix`, built with `rustPlatform.buildRustPackage` | nixpkgs maintainers |
| winget | a first manifest from `komac` or `wingetcreate` (portable zip), as a pull request to microsoft/winget-pkgs; later ones can be automated from `release.yml` | the release workflow, once added |
| Scoop | a bucket of our own with the Windows zip; the main bucket asks for more popularity | the release workflow, once added |
| Debian, Ubuntu, Fedora | each packages every crate separately, the 21 tabnas crates among them, so these follow the distributions' Rust teams | their Rust teams |

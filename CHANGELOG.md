# Changelog

Every notable change to aless, newest first. The format is
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions
follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

A release's notes on GitHub are its section here: the pull request that
bumps the version renames `Unreleased` to the version and its date
([RELEASING.md](RELEASING.md)).

## Unreleased

The first release.

### Added

- A [jless](https://jless.io)-style terminal viewer for JSON, JSON Lines,
  jsonic, JSONC, JSON5, YAML, TOML, INI, CSV, TSV, XML, ZON, Markdown and
  RSS/Atom feeds, with tabs, panes, a file explorer, regex search, and a
  watch mode that reloads a changed file and keeps your place.
- Custom grammars: any text format described in ABNF (`--grammar`,
  `--grammar-expr`).
- Without a screen, JSON for scripts and agents: `--json`, `--paths`,
  `--find`, `--where` and `--check`, each with source positions.
- `--render` to CSV, JSON, or any format whose tabnas crate carries a
  render, and `--alchemy` programs over any input.
- Releases: binaries for Linux (x86_64 and aarch64, glibc and static
  musl), macOS (x86_64 and aarch64) and Windows (x86_64 and aarch64), shell
  and PowerShell installers, a Homebrew tap, `cargo install` and
  `cargo binstall`, with checksums, a CycloneDX SBOM and build-provenance
  attestations.

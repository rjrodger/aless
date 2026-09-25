# CLAUDE.md

This repository is **aless**: a jless-style terminal viewer, written in
Rust, for every format the [tabnas](https://github.com/tabnas) parsers
read — with tabs and a watch mode that reloads a changed file while
keeping the reader's place. Start with [README.md](README.md) for what it
does, the key map and the module map; [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md)
records what it borrows from jless and from the AQL aless.

## Working on this repository

- **Build and test** from the repo root; the gates CI runs are exactly:
  ```bash
  cargo fmt --all --check
  cargo clippy --all-targets --locked -- -D warnings
  cargo test --locked
  python3 scripts/pty-smoke.py target/debug/aless   # unix: drives the real binary in a pty
  ```
  `cargo test` builds the binary too. Windows and macOS are release
  targets, and CI tests all three; from Linux, `cargo check --target
  x86_64-pc-windows-msvc` and `--target aarch64-apple-darwin` (after
  `rustup target add`) catch platform-specific compile errors early.
- **Dependencies come from GitHub, pinned by `Cargo.lock`.** The tabnas
  crates are not yet published, so `Cargo.toml` names each repository
  with `git = …`, and a `[patch."<repo>"]` table per grammar repository
  redirects the sibling `path = "../../parser/rs"` dependencies those
  manifests carry. `cargo update -p tabnas` (or any grammar) moves a pin
  to that repository's current default branch. When the crates reach
  crates.io: replace `git` with a version requirement and delete the
  patch tables; nothing else changes. Do not vendor or copy tabnas code
  into this repository.
- **Layout.** `src/main.rs` is the only file that touches the terminal
  (crossterm) and the only one with platform-specific code. Everything
  in the library is terminal-free and unit tested: `doc` (the arena
  model and rows), `fmt` (text forms, previews, JSON output, paths),
  `load` (format detection, the grammars, errors), `prov` (source
  positions by token alignment), `search`, `tab` (view state,
  navigation, reload re-anchoring), `app` (modes, key map, commands,
  tabs, watch scheduling), `render` (the screen as styled lines),
  `watch` (notify wrapper), `clip` (clipboard and OSC 52).
- **Behaviour is jless's unless the README says otherwise.** Keep the
  key map compatible: new bindings go on keys jless leaves free. A change
  to how something is drawn needs its `render` test updated; a change to
  navigation needs its `tab` test.
- **Tests** live next to the code (`#[cfg(test)]`) and in `tests/`
  (`formats.rs` loads every fixture, `app_flow.rs` runs the app
  headlessly against real files). Fixtures are under `tests/fixtures/`,
  one per format at least. Reload tests write to `std::env::temp_dir()`.
- **Every transient task reports progress**: long commands print a line
  at least every 30 seconds (cargo's own output counts).

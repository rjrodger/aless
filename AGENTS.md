# AGENTS.md

This repository is **aless**: a jless-style terminal viewer, written in
Rust, for every format the [tabnas](https://github.com/tabnas) parsers
read — with tabs and a watch mode that reloads a changed file while
keeping the reader's place — and, without a screen, a CLI that prints
JSON for scripts and agents. Start with [README.md](README.md) for what
it does, the key map, the agent interface and the module map;
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) records what it borrows
from jless and from the AQL aless. This file is the guide for any agent
working here; `CLAUDE.md` imports it.

## Working on this repository

- **Build and test** from the repo root; the gates CI runs are exactly:
  ```bash
  cargo fmt --all --check
  cargo clippy --all-targets --locked -- -D warnings
  cargo test --locked
  python3 scripts/pty-smoke.py target/debug/aless   # unix: drives the real binary in a pty, and checks it refuses without one
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
- **The dev profile turns the engine's debug assertions off**
  (`[profile.dev.package.tabnas]` in `Cargo.toml`). With them on, the
  engine compares its whole rule stack with a shadow copy on every step,
  an O(depth) check per step that a deep rule stack pays on every one of
  them: while tabnas-bnf spelled a repetition as a rule calling itself
  per item (the history the next point tells), a custom grammar's rule
  stack grew with the input and 800 lines of `hosts` took 31 s in a
  debug build, 0.4 s without the check. The check is the engine's own
  self-test, not aless's. `cargo test` and `target/debug/aless` inherit
  the setting; keep it.
- **A repetition is a replace loop, never a push chain.** In the engine an
  alternate that hands control to another rule either pushes a child
  rule (`p`), which opens a new frame for something the tree must nest,
  or replaces the current rule (`r`), which re-enters it in the same
  frame for the next item of a sequence. Every `*A`, `1*A` and `m*A` in
  a grammar compiles to a replace loop, the loop `r` and the item `p`
  where it nests, so the loop's iterations add no depth: a grammar's
  real recursion still nests with its input, but depth never grows with
  a file's length. That is the maintainer's rule for the whole tabnas
  fleet, and `load` is written to it: the shared cap of 3,000 open rules
  (`MAX_RULE_DEPTH` in `src/load.rs`) is a nesting guard, for a grammar
  from the command line as for the built-in ones, and a flat file of any
  length stays far under it. tabnas-bnf has compiled the star so since
  tabnas/bnf#80, which `Cargo.lock` pins. Until then its `desugar`
  spelled `*entry` as `H = inner H / ε`, a rule that calls itself once
  per item, 1,500 lines of `hosts` were "nested deeper than aless
  reads", and aless carried a workaround (commit 3ed798c): a
  `Format::Custom` parse ran under a cap of 1,000,000 open rules, with
  nesting measured on the built value instead. The cap is gone; the
  value measure stays (`MAX_VALUE_DEPTH`), since a custom grammar builds
  its value in the compiler's shape, an object and a `kids` array per
  level, which its rule stack does not count in the same units. Never
  write a repetition as a rule that calls itself in a grammar of this
  repository's own. The observable is the engine's rule depth `d`, what
  the guard in `load` reads as `ctx.rule_stack.len()`:
  `a_custom_grammars_repetition_is_not_nesting` holds 2,000 lines to
  the depth of two, and a 10,000-line `hosts` file to the depth of
  three. Rule depth over a repetition is constant; a test that repeats
  an item ten thousand times and asserts the maximum `d` stays what a
  single item needs is the proof.
- **Layout.** `src/main.rs` is the only file that touches the terminal
  (crossterm) and the only one with platform-specific code. Everything
  in the library is terminal-free and unit tested: `doc` (the arena
  model and rows), `fmt` (text forms, previews, JSON output, paths),
  `load` (format detection, the grammars, errors, the size, depth and
  time limits, the parse's own thread), `headless` (the agent
  interface: paths, listings, search, positions, checks, programs, JSON
  errors),
  `alchemy` (programs in the alchemy language: compiled, explained, and
  run over the input through `export`'s plumbing), `explorer` (directory
  trees as documents, listed lazily), `prov`
  (source positions by token alignment), `search`, `tab` (view state,
  navigation, reload re-anchoring), `app` (modes, key map, commands,
  tabs, watch scheduling), `render` (the screen as styled lines),
  `watch` (notify wrapper), `clip` (clipboard and OSC 52).
- **Behaviour is jless's unless the README says otherwise.** Keep the
  key map compatible: new bindings go on keys jless leaves free. A change
  to how something is drawn needs its `render` test updated; a change to
  navigation needs its `tab` test.
- **Tests** live next to the code (`#[cfg(test)]`) and in `tests/`
  (`formats.rs` loads every fixture, `app_flow.rs` runs the app
  headlessly against real files, `agent.rs` runs the built binary the way
  an agent does). Fixtures are under `tests/fixtures/`, one per format at
  least. Reload tests write to `std::env::temp_dir()`.
- **The agent interface is a contract.** The output shapes (entries,
  listings, `--check` reports, `{"error": …}` objects) and the exit
  statuses are documented in four places that must agree with the code
  and each other: the README's "Scripts and agents" section, the
  `WITHOUT A SCREEN` part of `--help` in `src/main.rs`,
  [`skills/aless/SKILL.md`](skills/aless/SKILL.md), and the tests in
  `src/headless.rs` and `tests/agent.rs`. Fields may be added; never
  rename, remove or repurpose one. Headless runs never block and never
  draw: no reading a terminal's standard input, no viewer without a
  terminal (it refuses with status 2, writing nothing to standard
  output), and nothing on standard output but the answer.
- **To look at a file while working here**, use aless itself:
  `target/debug/aless --paths --depth 1 FILE`, `--json --path P FILE`,
  `--where --at LINE:COL FILE`; see [the skill](skills/aless/SKILL.md).
- **Every transient task reports progress**: long commands print a line
  at least every 30 seconds (cargo's own output counts).

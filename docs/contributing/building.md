# Building and checking aless

aless builds with Cargo alone, from the repository root. The gates below
are what CI runs on every push and pull request; run them before a
push.

## The gates

The Rust gates:

```sh
cargo fmt --all --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
python3 admin/scripts/pty-smoke.py target/debug/aless
cargo package --locked
```

`cargo test` builds the binary too, and takes about three minutes, the
site build and its console examples included. The pty smoke test runs
on Unix: it drives the real binary in a pseudo-terminal, and checks that
it refuses to start without one. `cargo package` runs on Linux in CI: the
crate builds from exactly the files it publishes (`--allow-dirty` before
a commit).

The documentation and comment gates, which need Node.js 18 or later and
Vale:

```sh
node admin/scripts/comment-gate.cjs
node admin/scripts/prose-gate.cjs
vale --minAlertLevel=error $(node admin/scripts/prose-gate.cjs --list)
node admin/scripts/vale-counts.cjs
```

`DEV-PROCESS.md` says what the comment gate checks, and `STYLE-GUIDE.md`
what the prose gate and Vale check. CI pins Vale's version as
`VALE_VERSION`, and `vale sync` fetches the Google package `.vale.ini`
pins. `git config core.hooksPath .githooks` runs the comment gate before
each push.

A long command prints a line at least every 30 seconds, so a run is
never silent; Cargo's own output counts.

## Platforms

Linux, macOS and Windows are release targets, and CI tests all three.
`cargo check --locked --target x86_64-pc-windows-msvc` and
`--target aarch64-apple-darwin` catch a platform's compile errors before
CI does. They need each target's standard library, which rustup adds
(`rustup target add x86_64-pc-windows-msvc`); a Rust installed without
rustup builds for its own platform alone.

## Dependencies

Every dependency comes from crates.io, pinned by `Cargo.lock`. The
engine's package is `tabnas-parser`, and its library keeps the name
`tabnas`, so its line reads
`tabnas = { package = "tabnas-parser", version = … }`; every grammar is a
plain version requirement. `cargo update -p tabnas-parser`, or any of
them, moves a pin to the newest release the requirement allows. No
tabnas code is copied into this repository.

The dev profile turns the engine's debug assertions off
(`[profile.dev.package.tabnas-parser]` in `Cargo.toml`). With them on,
the engine compares its whole rule stack with a shadow copy at every
step, a cost that grows with the stack's depth. It is the engine's own
self-test, and `cargo test` and `target/debug/aless` inherit the
setting.

## The generated files

Every option is in `cli::OPTIONS`. The parser, `cli::parse_args`,
refuses an option the table does not list, and `-h`, `--help`, the man
page and the completions are written from it. A new option goes into
the table, with its summary and detail, and is read in `parse_args`;
`every_option_in_the_table_is_read` fails when it is not.

`man/aless.1` and `completions/` are the committed output of
`admin/scripts/generate.sh`, which a release archive and the crate
carry. Run it after changing an option, the reference or the version;
`the_generated_files_are_current` in `tests/binary.rs` fails until it
runs.

## The published crate

The crate carries what `Cargo.toml`'s `include` names: `src/`, the
licence files, the README and the changelog, the release and packaging
guides, the man page, the completions, and the example grammars under
`tests/fixtures/`. A file the build reads is under `src/` or on that
list, and so is every file the README links to by a relative path; the
README links anything else by its address on GitHub.

## Releases

Releases are dist's. `dist-workspace.toml` configures them, and dist
generates `.github/workflows/release.yml` from it: the workflow is never
edited by hand, but regenerated with `dist generate` at the version the
configuration names, and a pull request's `plan` job fails when the two
disagree. `publish-crates.yml` and `publish-homebrew.yml` are this
repository's own, and dist calls both. A release is a workflow dispatch
that creates its own tag; [RELEASING.md](../../RELEASING.md) has the
steps, and [PACKAGING.md](../../PACKAGING.md) what a packager needs.

# Building and checking aless

aless builds with Cargo alone. The build runs from the repository root.
The gates are the checks CI runs on every push and every pull request.
Run them before a push, so that a failure shows on your machine first.

## The gates

Run the Rust gates:

```sh
cargo fmt --all --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
python3 admin/scripts/pty-smoke.py target/debug/aless
cargo package --locked
```

`cargo test` builds the binary too. `cargo test` takes about 3 minutes.
That time includes the site build and its console examples.

The pseudo-terminal (pty) smoke test runs on Unix. It drives the real
binary in a pseudo-terminal. It also checks that aless refuses to start
without a terminal.

`cargo package` runs on Linux in CI. It checks that the crate builds
from the published files alone. To run `cargo package` before a
commit, add `--allow-dirty`.

After you install Node.js 18 or newer and Vale, run the documentation
and comment gates:

```sh
node admin/scripts/comment-gate.cjs
node admin/scripts/prose-gate.cjs
vale --minAlertLevel=error $(node admin/scripts/prose-gate.cjs --list)
node admin/scripts/vale-counts.cjs
```

`DEV-PROCESS.md` says what the comment gate checks. `STYLE-GUIDE.md`
says what the prose gate and Vale check.

CI pins the version of Vale as `VALE_VERSION`. `vale sync` fetches the
Google package that `.vale.ini` pins.

To run the comment gate before each push, run
`git config core.hooksPath .githooks`.

A long command prints a line at least every 30 seconds, so a run is
never silent. A line that Cargo itself prints counts as one of those
lines.

## Platforms

Linux, macOS, and Windows are release targets. CI tests all three. To
catch a platform's compile errors before CI does, run
`cargo check --locked --target x86_64-pc-windows-msvc` and the same
command with `--target aarch64-apple-darwin`.

A check for another target needs that target's standard library. To
add the library, run `rustup target add` with the target's name, for
example `rustup target add x86_64-pc-windows-msvc`. A Rust installed
without rustup builds for its own platform alone.

## Dependencies

Every dependency comes from crates.io. `Cargo.lock` pins each one. No
tabnas code is copied into this repository.

The engine's package is `tabnas-parser`. Its library keeps the name
`tabnas`, so the engine's line in `Cargo.toml` reads
`tabnas = { package = "tabnas-parser", version = … }`. Every grammar's
line is a plain version requirement. `cargo update -p tabnas-parser`
moves the engine's pin to the newest release the requirement allows.
The same command with another package name moves that package's pin.

The dev profile turns the engine's debug assertions off. The setting is
under `[profile.dev.package.tabnas-parser]` in `Cargo.toml`. With the
assertions on, the engine compares its whole rule stack with a shadow
copy at every step. The cost of that comparison grows with the stack's
depth. The comparison is the engine's own self-test. `cargo test` and
`target/debug/aless` inherit the setting.

## The generated files

Every option is in the table `cli::OPTIONS`. The parser is
`cli::parse_args`. It refuses an option the table does not list. aless
writes `-h`, `--help`, the man page, and the completions from the same
table.

To add an option:

1. Put it in the table, with its summary and detail.
2. Make `parse_args` read it. `every_option_in_the_table_is_read`
   fails until `parse_args` does.
3. Run `admin/scripts/generate.sh`.

`man/aless.1` and `completions/` are the committed output of
`admin/scripts/generate.sh`. A release archive and the crate carry
both. Run the script after a change to an option, the reference, or
the version. `the_generated_files_are_current` in `tests/binary.rs`
fails until the script has run.

## The published crate

The crate carries what the `include` list in `Cargo.toml` names:
`src/`, the licence files, the README and the changelog, the release
and packaging guides, the man page, the completions, and the example
grammars under `tests/fixtures/`. A file the build reads must be under
`src/` or on that list. So must every file the README links to by a
relative path. The README links anything else by its address on
GitHub.

## Releases

dist makes every release. The file `dist-workspace.toml` configures the
releases. From that file, dist generates
`.github/workflows/release.yml`. Never edit the workflow by hand: a
pull request's `plan` job fails when the workflow and the configuration
disagree. To regenerate the workflow, run `dist generate` at the version
the configuration names.

`publish-crates.yml` and `publish-homebrew.yml` are this repository's
own. dist calls both.

A release is a workflow dispatch. The dispatch creates its own tag.
For more information about the steps of a release, see
[Releasing aless](../../RELEASING.md). For more information about what
a packager needs, see [Packaging aless](../../PACKAGING.md).

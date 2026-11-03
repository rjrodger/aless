# Handover: CI for the pure viewer, housekeeping, and numbers as spelled, 2026-11-03

This is the running handover page between sessions working on aless. It
was last written on 2026-11-03, at the end of the session that took up
the 0.6.0 page's open list: the stale CI workflow, the yanked crate, the
stale link text, `__pycache__`, and numbers shown as doubles. The
handover before 0.6.0, on the streaming plan, left this repository when
the streaming exports did (aless#96) and is kept in tabnas/admin as
[`notes/2026-10-23-aless-streaming-handover.md`](https://github.com/tabnas/admin/blob/main/notes/2026-10-23-aless-streaming-handover.md);
the 0.6.0 session's fleet-wide record is admin's
[`notes/2026-11-02-aless-pure-viewer-handover.md`](https://github.com/tabnas/admin/blob/main/notes/2026-11-02-aless-pure-viewer-handover.md)
(tabnas/admin#161). This page is aless's own view: what aless is now,
what was decided, what is open here, and how to work here.

Read [AGENTS.md](../../AGENTS.md) first; it is current. Nothing published
(the README, the site, the changelog, `--help`) may cite this page.

## Where aless stands

- **0.6.0 is the latest release** (2026-11-02, `4cf5e05`, tag `v0.6.0`):
  the GitHub Release with every asset, the crate on crates.io, and the
  Homebrew tap's formula. `main` is at `71febd8` (aless#101, this page's
  previous version).
- **Four pull requests wait for the maintainer**, every local gate green.
  This session could not merge them (below). Merge them in this order:
  1. **aless#102**, CI for the pure viewer. Its CI is green on Linux,
     macOS and Windows, the first full run since 0.5.0, so 0.5.0's and
     0.6.0's code passes its tests on macOS and Windows too.
  2. **aless#103**, housekeeping: `clipboard-win` 5.4.3, the link text,
     `__pycache__/` ignored.
  3. **aless#104**, a number shown as its source spells it, where the
     spelling is JSON's. Under `Unreleased` in the changelog.
  4. **aless#105**, this page.

  The CI of #103, #104 and #105 fails until #102 is in `main`: a pull
  request runs the workflow of its merge with `main`, which still has
  the stale one. Once #102 is merged, update each branch (rebase on
  `main`, or GitHub's "Update branch") and its CI runs the new workflow.
- **aless is a viewer and nothing else.** It draws a document on a
  terminal and reads keys from it. Without a terminal (standard output a
  pipe or a file, no terminal to open, or `TERM=dumb`) it refuses to
  start, before reading any input, with one line on standard error and
  the exit status 2; nothing but `-h`, `--help`, `-V` and `--generate`
  writes to standard output. The exit statuses are 0 (quit), 1 (the
  terminal failed while the viewer ran) and 2 (it could not start).
- **The only JSON it writes** is `:w FILE` (a tab's document) and what
  the `y` keys copy. Reading these formats in a script, writing a
  document as another format and running a program over one are
  [dmog](https://github.com/tabnas/dmog)'s work; aless depends on none of
  alchemy, transduce or render.
- **Rust only.** The three scripts (`scripts/pty-smoke.py`,
  `scripts/homebrew-formula.py`, `scripts/generate.sh`) are tooling, and
  the site is Markdown a Rust test builds.

## The maintainer's rulings that shaped this state

On 2026-10-24:

1. **Correctness and architectural uniformity come first, not
   performance.** A library definition or a format's part has one
   implementation, its alchemy text, in every runtime alike (admin
   ADR-31). aless had carried Rust-only stand-ins; they went with the
   fleet's "one implementation" wave.
2. **Remove all streaming from aless and move it into alchemy-cli** (now
   dmog, admin ADR-32). aless#96 did it; aless 0.5.0 released it.

On 2026-11-02:

3. **Is streaming removed? Is it a TUI for data format inspection? Is it
   Rust only? What is missing?** Asked and answered as an assessment.
4. **Are the output options needed anymore?** The session advised
   keeping them as the agent face of the viewer's inspection; the
   maintainer ruled instead:
5. **Make aless a pure viewer.** aless#99 did it; aless#100 released
   0.6.0.

On 2026-11-03:

6. **Implement the handover**: the 0.6.0 page's "Open, in aless".
   aless#102, #103 and #104 are that work.

Ruling 5 is not yet an ADR in tabnas/admin. ADR-31 and ADR-32 cover one
implementation and the rename; if the pure viewer is to be a recorded
decision, it takes the next free number there.

## What the pure-viewer change did (aless#99, `c49d4d6`)

- Removed the output options `--json`, `--paths`, `--find`, `--where`,
  `--check`, `--path`, `--at`, `--limit` and `--compact`, the JSON error
  objects, the exit statuses 3 to 6, the Agent Skill (`skills/`,
  `--generate skill`), `src/headless.rs` (about 2,200 lines) and
  `tests/agent.rs` (about 2,000 lines). About 6,500 lines went and 1,250
  came.
- `src/main.rs`: no output branch; `no_screen()` refuses before any
  input or grammar file is read; `refuse_usage` and `refuse_viewer` print
  one line and exit 2; a grammar that does not compile or cannot be read
  exits 2 too.
- `src/cli.rs`: the option table has `Input`, `Viewer` and `About`
  sections; the reference's topics are the viewer, the three option
  groups, input and formats, keys, custom grammars, large inputs and
  limits, exit status, environment, files, examples and see also (jless,
  jq, the site, the source, tabnas/abnf, dmog). `man/aless.1` and
  `completions/` were regenerated by `scripts/generate.sh`.
- Tests: `tests/binary.rs` runs the built binary (`--version`, `-h`,
  `--help`, the generated files are current, the refusal without a
  terminal with nothing on standard output, usage errors, `--help |
  head`); `tests/grammars.rs` holds every grammar of
  `tests/fixtures/grammars/` to its `NAME.expected.json` through the
  library (`grammar::register`, `load::load_path`,
  `fmt::to_json_pretty`), with the thousands-of-lines, shadowing,
  non-UTF-8 path and compile-failure cases. The registry is one per
  process, so each test registers under names of its own or the
  fixtures'. `every_option_in_the_table_is_read` stays in `src/main.rs`.
- `scripts/pty-smoke.py` expects the refusal in every no-terminal case,
  `TERM=dumb` included; `scripts/homebrew-formula.py`'s test checks
  `--version`, `--generate man` and the refusal.
- Documentation: the README lost "Scripts and agents" and points at dmog;
  the site lost the scripting tutorial, the agents, check and extract
  guides and the command-line explanation; its how-to guides are two
  groups, "Setting up" and "In the viewer"; the home page, the format
  guide, the grammar guide and the explorer guide carry fresh captures
  taken in a pty at 80 columns. `Cargo.toml`'s description, the tagline
  and the man page's description describe a viewer.

### Decisions the session made within ruling 5

Not rulings; revisit with the maintainer if wanted.

- Every startup failure is status 2, a grammar file that cannot be read
  or is over `--max-size` included, where the output face gave it 3 or 5.
- `TERM=dumb` refuses, where it printed the document as JSON.
- The refusal names its cause, points at `aless --help`, and names dmog.
- `:w FILE` and the copy keys stay.

## What this session did (ruling 6)

| Pull request | Branch | What |
|---|---|---|
| aless#102 | `claude/ci-pure-viewer` | `ci.yml` as #99's description gave it: the fixtures and translation-matrix steps go, and the last step checks the refusal (status 2, nothing on standard output, `needs a terminal` on standard error). |
| aless#103 | `claude/housekeeping-0.6` | `cargo update -p clipboard-win` (5.4.1, yanked, to 5.4.3); `one-tree.md`'s link text; `__pycache__/` in `.gitignore`. |
| aless#104 | `claude/numbers-as-spelled` | A number keeps its source's spelling where the spelling is JSON's: in the tree, a collapsed preview, search, `:w` and the copy keys. |

### Numbers as spelled (aless#104)

- **`fmt::spelling(text, n)`** keeps a spelling that is a JSON number
  (RFC 8259), denotes `n` bit for bit, and is not what `fmt::number`
  writes: `1.0`, `-0`, `1e21`, `1E3`, `2.50` and integers past 2^53. A
  short integer returns at once, and a decimal `fmt::number` writes as
  spelled before the text is read back.
- **The spelling rides on the token**: `TokVal::Num(f64,
  Option<Box<String>>)`, a thin box, so `Tok` stays 48 bytes (a
  `Box<str>` field makes it 64). `prov::take_spelling` hands it to the
  node placed at the token only when the node holds that very number;
  `Doc::spellings` holds them by node, sparse.
- **`fmt::value_text(doc, id)`** is where every written value goes:
  `render`'s row, `fmt::preview`, `fmt::search_text` and
  `fmt::serialize`. `leaf_text(kind)` stays for a kind alone.
- **Measured**: a 7.2 MB array of 60,000 records loads about 3% slower,
  both when 120,000 of its numbers keep a spelling and when none does,
  against run-to-run noise of about 2% (release build, best of three,
  macOS aarch64). The figures are in #104's description.

### Decisions the session made within ruling 6

Not rulings; revisit with the maintainer if wanted.

- **A spelling is kept only where it is JSON's**, since `:w` and the copy
  keys write JSON. `0x1F` (JSON5), `1_000` (TOML), `+1`, `.5`, `5.` and
  `Infinity` are given as the value, as before.
- **CSV's column alignment hands no spelling on.** aless never turns on
  tabnas-csv's number lexing, so its cells are strings; a record that
  falls back to value matching goes through `align_nodes`, which does.
- **Numbers left the deviations from jless** (the README's and
  `site/explanation/jless.md`), since jless shows a JSON number as
  spelled too. The reference's sentence moved from LARGE INPUTS AND
  LIMITS to INPUT AND FORMATS, beside "Map keys keep their source order".
- **No release is cut.** #104 changes what the viewer shows; the bump
  that releases it, and when, is the maintainer's call.

## Open, in aless

- **Merge #102, then update and merge #103, #104 and #105** (above). In
  auto mode this session was refused a merge ("Merge Without Review"),
  and changing a pull request's base or closing and reopening it ("an
  unrequested commit in a connected app"), so the maintainer merges.
- **The Codex thread on aless#99** that found the stale CI step can be
  resolved once #102 is in `main`.
- **aless#29, the remote branches.** 60 branches predate this session.
  59 lose nothing if deleted: 58 are the heads of merged pull requests,
  each still reachable as `refs/pull/N/head`, and `wip/abnf-grammars` is
  in `main`'s history. One, `claude/handover-2026-10-02` (aless#41,
  closed unmerged, 5 commits of documentation not in `main`), is the
  maintainer's call. This session asked before deleting anything. The
  59, from a fresh fetch, are the remote branches whose pull request
  merged, plus `wip/abnf-grammars`:

  ```bash
  git fetch --prune origin
  gh pr list --state merged --limit 500 --json headRefName \
    --jq '.[].headRefName' | sort -u > /tmp/merged
  git branch -r --format='%(refname:lstrip=3)' | sort > /tmp/remote
  comm -12 /tmp/merged /tmp/remote | xargs git push origin --delete
  git push origin --delete wip/abnf-grammars
  ```

  An open pull request's branch is not in the merged list, so it is left
  alone, and `claude/handover-2026-10-02`'s pull request was closed, not
  merged. Turning on "Automatically delete head branches" in the
  repository's settings keeps them from piling up again. Then #29 can
  close.
- **A number placed by a best-effort alignment** (YAML, XML, Markdown)
  takes its spelling from the token it matched by value, so a misplaced
  match would show another spelling of the same float, and for an
  integer past 2^53, other digits. It is exact for the JSON family,
  TOML, INI and ZON. No case of it was found.
- **The Windows and macOS `cargo check --target` gates** were not run
  locally: the maintainer's machine has Homebrew's Rust, without rustup.
  CI's three platforms run them (green on #102).

## Open, around aless

- **The pure-viewer ruling has no ADR** (above).
- **`tabnas/dmog` does not exist on GitHub yet**: the repository is
  still `tabnas/alchemy-cli`, and `https://github.com/tabnas/dmog`
  answers 404 (checked 2026-11-03). aless's README, changelog, site and
  `--help` link to it five times, so those links are broken until the
  repository is renamed. Renaming it fixes them and leaves a redirect
  for the old name.
- **dmog's gaps, flagged when streaming moved:** a streamed export for a
  custom grammar has no home there (it would need all three runtimes);
  `translate` has no `--timeout` or `--max-size`, and `--from` is
  required; the YAML round trip test is Rust only.
- **admin's daily `port-deps` check has failed since 2026-10-31:**
  alchemy-cli's ports name the repository `dmog` while the job's clone is
  named `alchemy-cli`, and `docs/deps/deps.json` is stale with it. A
  pull request to admin meets that red check whatever it changes. The
  rename above likely settles it.
- Carried over from the streaming handover, now dmog's and the parts'
  concern: TypeScript and Go reading a reader a window at a time (step
  10's item 7), their steps 12 and 13, `lex.window` in the other plugins,
  parser#258, admin's citations of parser's removed handover page.

## How to work here

- **The gates**, before every push, from the repository root:
  `cargo fmt --all --check`, `cargo clippy --all-targets --locked -- -D
  warnings`, `cargo test --locked` (about three minutes, the site build
  with its console examples included), `python3 scripts/pty-smoke.py
  target/debug/aless`, `cargo package --locked` (`--allow-dirty` before
  the commit), and `cargo check --locked --target` for
  `x86_64-pc-windows-msvc` and `aarch64-apple-darwin` where rustup is
  installed.
- **After an option or reference change**, `scripts/generate.sh`;
  `the_generated_files_are_current` fails until it runs.
- **Screen captures for the site** come from a pty at 80 columns. A
  small script that loads `scripts/pty-smoke.py` with `importlib`, forks
  aless in a pty of the wanted size, feeds keys and prints
  `Screen.text()` does it; piped input goes through a pipe on standard
  input. Capture from a directory whose path reads well, since the
  status bar shows it.
- **A release** follows RELEASING.md exactly: the bump in its own pull
  request (manifest, `cargo update --workspace`, the changelog heading,
  `scripts/generate.sh`, PACKAGING.md's `pkgver`, `version` and URLs),
  merge, record main's SHA, dispatch `release.yml` with `dry-run` (about
  eleven minutes), then with the tag (about thirteen), then confirm the
  Release by tag, crates.io and the tap's raw formula. 0.5.0 and 0.6.0
  were both made this way.
- **To measure a change's cost**, time `load::load_path` on a generated
  file in a test of its own (`cargo test --release --test NAME --
  --nocapture`), in this checkout and in a detached worktree of `main`,
  alternating, best of three. Delete the test after.
- **Edit with scripts that fail loudly**: Python with an exact-match
  `rep(old, new)` that asserts one occurrence, heredocs for whole files,
  `sed` for one-line changes.
- **What a session can do depends on where it runs.** On the
  maintainer's machine git pushes over SSH and `gh`'s token has the
  `workflow` scope, so a workflow file can be pushed (#102 was). A cloud
  session's credentials cannot push a workflow file or a tag, amend, or
  force-push; there, a follow-up commit does what an amend would. In
  auto mode no session merges, retargets, closes or reopens a pull
  request: the maintainer does. Keep `rm -rf` out of compound commands
  and never of the directory you are in.
- **To look at a file while working here**, an agent has no terminal to
  run aless in: `dmog translate --from FORMAT --to json FILE` prints it
  as JSON, and `jq` takes it from there. dmog was not installed on the
  maintainer's machine on 2026-11-03; `load::load_str` in a throwaway
  test does the same through aless's own loader.

## Leaving this page current

Before a session ends, rewrite the date in the title, "Where aless
stands", the open lists and anything above they contradict. A dated
record of a session that changed more than aless belongs in tabnas/admin
under `notes/`; this page stays aless's own.

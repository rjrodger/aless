# The documentation site

The site at `https://aless.tabnas.dev`, `Cargo.toml`'s `homepage`, is the
Markdown under `docs/`, built by a Rust test. `tests/site.rs` renders the
pages with tabnas-markdown into `docs/layout.html`, checks the result,
and with `ALESS_SITE_OUT=DIR` writes it into `DIR`, which
`.github/workflows/pages.yml` publishes from `main`:

```sh
ALESS_SITE_OUT=/tmp/aless-site cargo test --test site
```

## Pages and their kinds

A directory for each of Diátaxis's kinds: `tutorials/` (lessons, followed
from start to end), `how-to/` (one task each), `reference/` and
`explanation/`. Each directory's `index.md` is navigation, as are
`docs/index.md` and `docs/404.md`. `STYLE-GUIDE.md` names pages by file
name; these predate that layout, so a page's kind is its directory.

The pages under `docs/contributing/` are contributor pages, for someone
working on aless rather than with it. The site build does not publish
them.

## Front matter

A page starts with `title`, `description` (one sentence) and, for a page
in its section's list (every page of a section but its index), `order`,
its place in the list. The how-to guides are listed by group, so each
also names its `group`, the heading it is under: "Setting up" or "In the
viewer". The layout writes the page's one `h1`, so the Markdown starts at
`##`. A value holds no colon followed by a space: Vale reads the front
matter as YAML, and the build reads each line as `key: value`.

## Links

Links between pages run from the site's root (`/how-to/search.html`),
and the build makes them relative, so the site reads the same from a
directory on disk as from its domain. The prose gate's link check reads
repository files, and these links name the built site's, so `docs/` is
not in its `links` list. `tests/site.rs` checks every link and every
`#anchor` of the built site instead, against the files it writes.

## What the build writes from elsewhere

The reference's pages are written from the binary, never by hand: the
command line from the topics `--help` prints (`cli::reference_html`), the
keys from the help F1 shows (`app::help_lines`), and the releases from
`CHANGELOG.md`. A new option or key reaches the reference by itself, and
`the_reference_is_the_binarys` holds the pages to the binary.

`{{version}}` and `{{tagline}}` in a page are filled in from the binary.
Every page is published beside its HTML as Markdown, at the same path
with `.md`, and `llms.txt` lists them.

The site is drawn as tabnas.dev is: `docs/style.css` carries the
design tokens of tabnas/web whole, and `docs/fonts/` its faces, so a page
requests nothing from anywhere else.

## Examples and captures

Output on a page is pasted from a real run, in a `console` fence with
`$ ` before each command. `tests/site.rs` runs every one on Unix, in a
directory of the files the site publishes under `/examples/` (the files
in `examples/` and the fixtures `FIXTURES` names), and fails when the
output differs from the page. Each command line is also held to the
options aless takes. This is the harness `STYLE-GUIDE.md` asks a project
to name; it reads `console` fences rather than test directives.

A capture of the viewer is a `screen` fence, 80 columns wide, taken in a
pseudo-terminal as `admin/scripts/pty-smoke.py` drives one: a small
script that loads it with `importlib`, starts aless in a pty of the
wanted size, sends keys and prints `Screen.text()`. Piped input goes
through a pipe on standard input. Capture from a directory whose path
reads well, since the status bar shows it.

## Style

Every page, the README and the changelog follow `STYLE-GUIDE.md`, and
the prose gate and Vale hold them to it. A published page cites nothing
internal and carries no project history; the changelog is the place
history is written.

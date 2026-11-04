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
also names its `group`, the heading it is under: "Setting up", "Reading
a document" or "Files and formats", the groups `SECTIONS` in
`tests/site.rs` declares. The layout writes the page's one `h1`, so the Markdown starts at
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
command line from the topics `--help` prints (`cli::reference_html`), and
the keys from the table F1 shows (`app::KEY_MAP`). The releases are
`CHANGELOG.md`. The formats page is written by hand, and
`the_formats_page_is_the_binarys` holds it to `Format::ALL`: a row for
every format, naming every extension. A new option, key or format
reaches the reference by itself, or fails a test until it does, and
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

A capture of the viewer is a `screen` fence, and every one is drawn by
aless, never pasted. The comment before it says how:

```markdown
<!-- screen: aless bookshelf-openapi.yaml
keys: j j h c / listBooks Enter
size: 80x14 -->
```

The first line is the command, as the reader would type it, or
`printf 'TEXT' | aless ...` for standard input. `keys:` is what is
typed after it opens, a word at a time: a key's name (`Enter`, `Esc`,
`Tab`, `S-Tab`, `Space`, `Backspace`, an arrow such as `Up`, `Home`,
`End`, `PgUp`, `PgDn`, `F1`, or `C-d`) is that key, and any other word
is typed as it is spelled. `size:` is the terminal, 80 columns by 12
rows unless it says otherwise. `rows: A-B` keeps those rows of the
screen alone, `edit: FILE: OLD => NEW` changes the file once the keys
are typed, and reloads it as the watcher would, and `cwd: ..` runs the
command from the directory above the files.

`the_screens_are_what_aless_draws` builds the app as the command line
would (`cli::parse_args`), types the keys, and compares what
`render::screen` draws with the fence, on every platform. It runs none
of the effects the terminal side would, so a capture shows no `Copied`
message, and it does not change directory, so a capture never writes a
file: `:w` there would write into the repository. The files sit
in a directory the pages show as `/home/user/bookshelf`; on Windows a
capture that shows that path is skipped. To write the fences from the
captures, after a change that moves what aless draws:

```sh
ALESS_SCREENS=write cargo test --test site the_screens
```

and read the diff before committing it. A screen block with no comment
before it fails the test.

Every other example in a tutorial, a how-to guide or a reference page
is run too, or says why not: `every_example_is_run_or_says_why_not`
fails on a block that is neither followed by the capture of one of its
commands nor preceded by the style guide's skip:

```markdown
<!-- test: skip it installs aless from crates.io -->
```

The reason is for the reviewer, so it names what keeps the test away:
an install, a download, a change to the reader's own files. The build
strips both kinds of comment from the pages it publishes.

## Style

Every page, the README and the changelog follow `STYLE-GUIDE.md`, and
the prose gate and Vale hold them to it. A published page cites nothing
internal and carries no project history; the changelog is the place
history is written.

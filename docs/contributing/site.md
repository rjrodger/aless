# The documentation site

The site at `https://aless.tabnas.dev` is the Markdown under `docs/`.
That address is `Cargo.toml`'s `homepage`. A Rust test, `tests/site.rs`,
builds the site: it renders each page with tabnas-markdown into
`docs/layout.html`, then checks the result. With `ALESS_SITE_OUT=DIR`
set, the test also writes the site into `DIR`, the directory you name.
From `main`, `.github/workflows/pages.yml` publishes the site that the
test writes into `DIR`. To build the site into a directory of your own,
run the test with the variable set:

```sh
ALESS_SITE_OUT=/tmp/aless-site cargo test --test site
```

## Pages and their kinds

A page's kind is its directory. `docs/` holds a directory for each of
the four kinds Diátaxis defines: `tutorials/`, `how-to/`, `reference/`,
and `explanation/`. A tutorial is a lesson, followed from start to end.
A how-to guide covers one task. Each directory's `index.md` is
navigation, and so are `docs/index.md` and `docs/404.md`.
`STYLE-GUIDE.md` tells a page's kind from its filename. The site's
pages predate that naming, so a page's directory tells its kind instead.

`why/` is none of the four. Its one page, `why/index.md`, is Richard
Rodger's own account of why aless exists. The page is human written.
The site shows a byline under its title, and no label above it.
The page names its `author` in its front matter. A page of any other
section must not. The page also sits outside the style guide:
`admin/scripts/prose-gate.json` names each directory it gates rather
than all of `docs/`, and leaves `why/` out.
`the_gates_read_every_page_but_the_human_ones` fails when a new
section's directory is missing from that list. The page's text changes
only when its author changes it. Its captures are drawn and checked like
any other page's.

The pages under `docs/contributing/` are contributor pages, for someone
working on aless rather than with it. The site build does not publish
them.

## Front matter

A page starts with `title` and `description`. The description is one
sentence. A page in its section's list also gives `order`, its place in
the list. The list holds every page of a section except its index.
The how-to guides are listed by group, so each guide also names its
`group`, the heading it is listed under.
The groups are `Setting up`, `Reading a document`, and
`Files and formats`. `SECTIONS` in `tests/site.rs` declares the groups.
A page of the human-written section names its `author` as well. The
layout writes the page's one `h1`, so the Markdown starts at `##`. A
value holds no colon followed by a space, because Vale reads the front
matter as YAML and the build reads each line as `key: value`.

## Links

A link between pages runs from the site's root, as `/how-to/search.html`
does. A link to a section's index names the section's directory, as
`/how-to/` does. A link to the home page is `/`. No link names
`index.html`, because the directory is the page's address.
`tests/site.rs` fails on a link that names `index.html`. The build makes each link
relative, so the site reads the same from a directory on disk as from
its domain. On disk, no server finds a directory's `index.html`, so the
layout's script adds it to each link to a directory. `docs/` is not in the prose
gate's `links` list, because the gate's link check reads repository
files and these links name the built site's files. Instead,
`tests/site.rs` checks every link and every `#anchor` of the built
site against the files it writes.

## Content from elsewhere

The build writes the reference's pages from the binary, never by hand.
Two of those pages come from these parts of the binary:

- The command-line page comes from the topics `--help` prints, through
  `cli::reference_html`.
- The keys page comes from the table `F1` shows, `app::KEY_MAP`.

The releases page comes from `CHANGELOG.md`. The formats page is the
exception: it is written by hand. `the_formats_page_is_the_binarys`
still holds it to `Format::ALL`: a row for every format, naming every
extension.
`the_reference_is_the_binarys` holds the reference's pages to the
binary. A new option, key, or format therefore reaches the reference by
itself, or fails a test until it does.

The build fills in `{{version}}` and `{{tagline}}` in a page from the
binary.

The build also publishes every page as Markdown. Each Markdown page sits
beside its HTML, at the same path with `.md`. `llms.txt` lists those
Markdown pages.

The site is drawn as tabnas.dev is: `docs/style.css` carries the design
tokens of tabnas/web whole, and `docs/fonts/` carries the faces of
tabnas/web. A page therefore requests nothing from anywhere else.

The build draws the favicon from `docs/logo.svg`. The favicon is the
logo's multitool without the wordmark, which no tab is wide enough to
read. The favicon takes up any change to the multitool's shapes and
colours, because the build draws it from the logo's own paths. Its crop
is fixed: `FAVICON_VIEW` in `tests/site.rs` is the multitool's box,
square, with a margin. A logo whose multitool moves or changes size
needs that box measured again. In a browser, remove `#aless-wordmark`
from the logo and read `getBBox()` of its root `<svg>`.

## Examples and captures

Output on a page is pasted from a real run. It sits in a `console`
fence. Each command in the fence starts with `$ `. On Unix, `tests/site.rs` runs
every `console` fence and fails when the output differs from the page.
The test runs the fences in a directory of the files the site
publishes under `/examples/`: the files in `examples/`, and the
fixtures `FIXTURES` names. It also holds each command line to the
options aless takes. This test is the harness `STYLE-GUIDE.md` asks a
project to name. It reads `console` fences rather than test directives.

A capture of the viewer is a `screen` fence. aless draws every capture,
so none is pasted by hand. The comment before the fence says what to
draw:

```markdown
<!-- screen: aless bookshelf-openapi.yaml
keys: j j h c / listBooks Enter
size: 80x14 -->
```

The first line is the command, as the reader would type it. For
standard input, the first line is `printf 'TEXT' | aless ...`, where
`TEXT` is what aless reads. Each line after the command is one of these
fields:

- `keys:` is what is typed once aless has opened, a word at a time. The
  key names are `Enter`, `Esc`, `Tab`, `S-Tab`, `Space`, `Backspace`,
  `Home`, `End`, `PgUp`, `PgDn`, `F1`, `C-d`, and an arrow such as
  `Up`. A word that names a key is that key. Any other word is typed as
  it is spelled.
- `size:` is the terminal. Without this line, the terminal is 80 columns
  by 12 rows.
- `rows: A-B` keeps only rows `A` to `B` of the screen.
- `edit: FILE: OLD => NEW` changes `OLD` to `NEW` in `FILE` once the
  keys are typed. The capture then reloads the file as the watcher
  would.
- `cwd: ..` runs the command from the directory above the files.

`the_screens_are_what_aless_draws` builds the app as the command line
would, through `cli::parse_args`. It types the keys, then compares what
`render::screen` draws with the fence. The comparison runs on every
platform. The test runs none of the effects the terminal side would, so
a capture shows no `Copied` message. The test does not change directory
either, so a capture never writes a file: `:w` there would write into
the repository. The files under `/examples/` sit in a directory the
pages show as `/home/user/bookshelf`. On Windows, the test skips a
capture that shows that path.

After a change that moves what aless draws, write the fences from
their captures:

```sh
ALESS_SCREENS=write cargo test --test site the_screens
```

Then read the diff before committing it. A `screen` fence with no
comment before it fails the test.

Every other example in a tutorial, a how-to guide, or a reference page
is run too, or says why not. `every_example_is_run_or_says_why_not`
fails on a fence that is neither followed by the capture of one of its
commands nor preceded by the style guide's skip comment:

```markdown
<!-- test: skip it installs aless from crates.io -->
```

The reason is for the reviewer, so it names what keeps the test away,
for example an install, a download, or a change to the reader's own
files. The build strips both kinds of comment, `screen` and `test`,
from the pages it publishes.

## Style

Every page, the README, and the changelog follow `STYLE-GUIDE.md`. The
prose gate and Vale hold them to it. A published page cites nothing
internal, because its reader has aless and not the repository. A
published page carries no project history either: the changelog is
where history is written.

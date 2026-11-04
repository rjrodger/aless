# Documentation style guide

How a project's documentation is written. This guide is normative for
every page in the project's gated set: the published documentation,
and the READMEs a reader meets first, as
[`admin/scripts/prose-gate.json`](admin/scripts/prose-gate.json) lists
them. It exists so that a page written next year sounds like a page
written this year, and so that a reviewer can point at a rule instead
of arguing taste.

Nothing here belongs to one project. What does (which pages there are
and what kind each is, the project's terms of art, and the commands
that run each gate) is in the project's agent guide, `AGENTS.md`, and
in the gates' configuration files.

Three sources feed it, in a fixed priority order. The same order is
encoded in `.vale.ini`, and every rule switched off there names the
reason:

    house voice  ->  Google  ->  Vale defaults

1. **This file.** Where it rules, it rules. The house voice is Richard
   Rodger's blog register, and the places it wins are listed with their
   reasons rather than left as silent exceptions: first-person plural in
   tutorials, British spellings, quotation punctuation outside the
   quotes, and the parenthesis ration.
2. The [Google developer documentation style
   guide](https://developers.google.com/style) for everything this file
   does not cover: second person, present tense, active voice,
   sentence-style capitalisation in headings, serial commas, one idea
   per sentence.
3. [Vale](https://vale.sh) defaults, which mostly means spelling.

## How this guide is enforced

Two gates check it, and they read one file list and one banned list so
that neither can drift from the other. The file list is
`prose-gate.json`; the banned list is `reject.txt` in the house
vocabulary, `.vale/styles/config/vocabularies/House/`. The project's
`AGENTS.md` names the command that runs each gate and where CI runs it.

| Gate | Checks |
|---|---|
| Vale, configured by [`.vale.ini`](.vale.ini) | spelling, Google's conventions, and the banned list, at the levels set in `.vale.ini` |
| [`admin/scripts/prose-gate.cjs`](admin/scripts/prose-gate.cjs) | the banned list again, across line wraps, the no-em-dash rule, the first-person rules, the exclamation ration, no emoji, no internal citations, the project's own spellings, that every internal markdown link resolves, and that this guide summarises every category of the banned list |
| [`admin/scripts/vale-counts.cjs`](admin/scripts/vale-counts.cjs) | that every count `.vale.ini` records is what Vale reports |

Vale takes its file list from
`node admin/scripts/prose-gate.cjs --list`, because Vale selects a
configuration section by file extension, not by path, so a section
cannot leave a directory out.

`prose-gate.json` holds what belongs to the project: `pages`, the
gated set, as globs; `tutorials`, the pages that may say "we" and spend
one exclamation mark; `contributor`, the pages exempt from the
internal-citation rule; `links`, the markdown files whose links are
checked; `skip`, the directories the gate never enters, `sketch/`
among them unless the project says otherwise; `names`, the project's
own spellings, each a `pattern` the gate refuses and the form it is
`spelled`; and `internal`, the project's own internal records, each a
`pattern` and `what` a match cites. `guide` and `vocabulary` name this
file and the vocabulary directory, where a project moves either. Each
finding names its rule:

| Rule | Fails when |
|------|-----------|
| `banned-phrase` | a phrase on the banned list appears in prose, even split by a line wrap |
| `em-dash` | an em dash appears in prose |
| `first-person-plural` | "we", "us", "our" or "let's" appears outside a tutorial |
| `first-person-singular` | "I", "my" or "mine" appears anywhere |
| `exclamation` | an exclamation mark appears outside a tutorial, or a second one inside one |
| `emoji` | an emoji appears anywhere in the page |
| `internal-citation` | a published page cites an internal record |
| `name-spelling` | a name is spelled a way `names` refuses |
| `dead-link` | a relative link names a file that does not exist, or a heading its file does not have |
| `guide-category` | a category of `reject.txt` has no summary in this guide |

Prose, to the gate, is the page without its fences, code spans, link
destinations, URLs, comments and frontmatter: literal code and quoted
output keep their punctuation.

The gated set is the reader-facing one: the published documentation
and the READMEs a reader meets. Design notes, trackers and the working
documents under `sketch/` are out. One list, read by both gates, is the
point: a page published from a directory the list does not name is
checked by neither, and nothing reports it.

**The link check reads more than the gated set.** It takes every
markdown file `links` names, working documents included, because a
renamed heading breaks links from wherever they were written. Only
`sketch/` is left out, because its files are ignored. It resolves each
link's file, and each `#anchor` against the target file's own headings
under GitHub's slug rules.

**A Google rule demoted below error level on evidence was tried at
error first and found wrong for these pages.** `.vale.ini` records, for
every rule demoted on evidence, what it produced on a clean run. Two
overrides turn on a reason rather than on taste:

- `Google.EmDash` is disabled because its spacing rule is redundant
  under the house ban. The prose gate checks prose after stripping code
  and quoted output.
- `Google.OxfordComma` cannot tell a two-item list sitting after a
  comma clause from a list. A rule that wrong cannot fail a build, and
  an author reading its output can tell the two apart in the page in
  front of them, so it is a warning.

**Those numbers are measured, not remembered.** A count typed from
memory goes stale the first time a page is edited, which is the one
thing a recorded count exists to prevent.
`node admin/scripts/vale-counts.cjs` reads every count in `.vale.ini`
against a live Vale run and fails on any difference; `--write`
re-measures, and a change to a page that moves a count runs it in the
same commit. A rule switched off is measured with it switched back on,
because the count is the evidence for switching it off.

## The structure: Diátaxis, enforced by placement

Every page is exactly one of four kinds, apart from the navigation
pages described under the table, and the kind decides what the page
may do. Where a page sits and what it is called say which kind it is:

| Kind | Files | May | May not |
|---|---|---|---|
| Tutorial | `docs/tutorial-*.md` | teach step by step, show output for every step, defer detail with a link | argue design, list every flag, assume the reader's goal |
| How-to | `docs/how-to/*.md` | solve one named task, assume competence, link the reference | teach basics, explain design, drift into a second task |
| Reference | `docs/reference-*.md` | state facts exhaustively and dryly, pin claims to tests | narrate, persuade, teach |
| Explanation | `docs/explanation*.md` | argue, compare, admit trade-offs, tell the design's story | be the only place a fact lives |

A project whose pages predate this layout lists each page's kind in its
`AGENTS.md`; the prose gate reads which pages are tutorials from
`tutorials` in `prose-gate.json`.

One fact appears in all four kinds at different altitudes (met in the
tutorial, used in a how-to, specified in the reference, argued in the
explanation) but the normative statement lives in the reference and
everything else links to it.

A page that lists a set of pages (`docs/tutorial.md` for the
tutorials, `docs/how-to/README.md` for the guides) is navigation rather
than one of the four kinds. It says what its set holds and where to go
when that set is the wrong shape, and it neither teaches nor argues, so
the relaxations a tutorial gets (the first-person plural, the one
exclamation mark) do not reach it.

An operator document dangerous enough that its rationale stays beside
its commands, such as a release procedure, is a deliberate exception.
Say so in the page rather than splitting it.

## The published set cites nothing internal

The documentation is written for someone who has the tool and not the
repository. Two sets of documents exist, and only one of them is
published:

| Set | Files | Audience |
|---|---|---|
| Published | the gated set, apart from its contributor pages | anyone using the project |
| Internal | `ADR.md`, `AGENTS.md`, `DEV-PROCESS.md`, this guide, `sketch/`, `admin/`, and the project's other records | contributors |

**A published page never cites an internal one.** Not as a link, not
as a parenthetical, not as a bare token. `(ADR-004)` after a rule tells
a reader nothing they can act on: the decision record argues a choice
already made, and the rule it decided is what the page is for. State
the rule and stop.

This runs both ways. A published page may not carry the project's own
history either: what a figure used to say, which release moved it,
which bug report prompted the wording. That belongs in the commit
message, the changelog or the decision record. A reader wants the
project as it is today.

**The program's own output is published text.** An error message that
names a design document sends a user somewhere they cannot go, in place
of telling them what to do. A message names the repair, not the
reasoning; the reasoning stays where a contributor reads it.

A name that is the product's own surface is not a citation: an error
code, or a filename that happens to look internal. For the same reason
the gate refuses a link to `AGENTS.md` and not its bare name, which a
tool may write or read as part of what it does.

The rule runs one way. Internal documents cite each other and cite the
documentation freely, because a decision record that does not show its
working is not a decision record. Only the direction out of the
published set is closed.

The contributor pages in the gated set are exempt, because their reader
is a contributor: the pages under `docs/contributing/`, an operator
document, and the **root `README.md`**, which is the repository's front
page and whose job includes pointing at `AGENTS.md`. A package's own
README is not exempt: a package registry renders it to somebody who has
the package and not the repository. `contributor` in `prose-gate.json`
lists the exempt pages.

The enforced subset is the prose gate's `internal-citation` rule: a
decision record by number, as "the ADR" or as `ADR.md`, a path into
`sketch/` or `admin/`, a design note, a numbered decision, and a link
to `AGENTS.md`, `CLAUDE.md`, `DEV-PROCESS.md` or this guide, with the
project's own records added under `internal`. Vale does not carry this
one, because the file sets differ.

## The voice

The house voice is Richard Rodger's blog register, adapted per
document kind. The portable part of that voice is its *rhythm*, not
its stock phrases. Ten habits, with the register they apply in:

1. **Open with a concrete fact or a plainly stated problem, then a
   short dry beat.** Tutorials and how-tos. Reference pages open by
   stating what the thing is.
2. **Introduce code with a short colon-terminated sentence**: "Write
   this as `config.json`:", "Now run it:". Never "The following code
   snippet demonstrates". Everywhere.
3. **After a code block, point at the one interesting thing.** Do not
   recap the code. Everywhere.
4. **Parentheses carry definitions, caveats, and at most one dry
   aside per page.** Tutorials and how-tos. In reference pages,
   parentheses carry facts only.
5. **State a trade-off in a separate clause or sentence.** Use a comma,
   parentheses, or a new sentence; punctuation should not supply drama.
6. **Alternate one long explanatory sentence with one short verdict
   sentence.** The short sentence is the payoff. Everywhere.
7. **Talk to the reader as "you", and route them** ("If you already
   know Make, skip to…"). "We" appears only in tutorials, walking
   through code together. "I" appears nowhere.
8. **Show that the code is real.** Every example is executed by the
   test suite; when a page says so, say it plainly ("this output
   comes from the program, not from the author's memory").
9. **Jokes are self-directed or about the industry's mundanity, and
   the register goes fully serious the moment correctness or safety
   is on the table.** Never joke about the reader, other tools, or an
   error's consequences.
10. **Close by handing the reader something**: a link, a next step,
    one sentence. No summary paragraphs that restate the page.

Exclamation marks: at most one per page, in tutorials only, on a
genuine payoff.

## Banned phrases and patterns

These read as generated filler. Do not use them, in any document,
including commit messages that quote the docs.

**The list itself lives in
`.vale/styles/config/vocabularies/House/reject.txt`**, one regular
expression per line. That file is the single source of truth: Vale
reads it, and `admin/scripts/prose-gate.cjs` reads the same file rather
than keeping a second copy, so the two gates cannot disagree about what
is banned. Add a phrase there and both pick it up. What follows is a
reader's summary of it, not a second list; every phrase is shown as
code so that quoting a banned phrase in this guide does not fail the
gate.

It draws on two sources: the original house list, and
[claudisms.ai](https://claudisms.ai/), a catalogue of the patterns that
mark machine-written prose.

**Filler and false emphasis**: `worth noting` · `important to note` ·
`it cannot be overstated` · `at its core` · `when it comes to` ·
`let's break it down` · `here's where it gets interesting` ·
`because it matters`.

**Inflated vocabulary**: `delve` · `dive into` · `robust` · `seamless` ·
`comprehensive` · `holistic` · `intricate` · `leverage` · `foster` ·
`shed light on` · `pave the way` · `pivotal` · `transformative` ·
`game-changing` · `cutting-edge` · `groundbreaking` · `testament to` ·
`paradigm shift` · `realm` · `landscape of` · `navigate` · `unpack` ·
`lean into` · `throughline` · `double-click on` · `mature setup`.

**Consultant register**: `north star` · `key takeaways` ·
`best practices` (name the practice instead) · `at the end of the day` ·
`pressure-test` · `right-size` · `strategic imperative` ·
`three things to know` · `dispatches from` · `best operators` ·
`lessons learned`.

**Metaphor inflation**: `load-bearing` · `heavy lifting` ·
`is doing the work` · `different physics` · `hits hardest` ·
`quietly` (say `silently`, which is the term of art for a failure that
reports nothing).

**The contrast frame and its cousins**: `not just` · `not only X but Y` ·
`it's not about` · `the whole game` · `the entire point` ·
`the only thing that matters`. Say what the thing is.

**False singularity and crowned superlatives**:
`the right way/answer/tool/question` · `the best thing you can do` ·
`if I had to pick` · `what struck me` · `stuck with me` ·
`struck a chord` · `hit a nerve` · `we've seen this movie`.

**Reflective pose**: `sit with` · `worth exploring/considering/asking` ·
`keeps coming back to` · `that's the tell` · `where I landed`.

**Invented observation about people**: `most people` ·
`everyone I've worked with` · `a lot of folks` · `nobody I know`. If it
did not happen, do not claim to have noticed it.

**Signposting**: `let's explore` · `now let's turn to` · `moving on to` ·
`in today's rapidly evolving` · `reflecting a broader trend` ·
`marking a significant shift` · `great question`.

The next five categories come from **AiTells**, a Vale package by Krishna
Sunkam, MIT licensed, at `github.com/krishnasunkam/vale-ai-tells`. They
are carried as patterns rather than as a package, so `reject.txt` stays
the one list both gates read, and each was checked against a sentence
it must catch.

**Inflated copulas**: `serves as a/the` · `stands as a` ·
`represents a significant` · `boasts a`. The copula is carrying weight a
plain verb should carry. Name what the thing does.

**Cliche**: `in a nutshell` · `low-hanging fruit` · `move the needle` ·
`thought leader` · `best-in-class` · `state-of-the-art` ·
`tip of the iceberg` · `win-win` · `secret sauce` ·
`double-edged sword` · `perfect storm` · `fast-paced world` ·
`ever-evolving`.

**Vague attribution**: `responsible for` · `duties included` ·
`in charge of` · `tasked with`. Say what the code does, not what it is
for.

**Nominalisation: the verb is hiding in the noun**: `conduct`,
`perform`, `undertake`, `carry out` or `engage in` followed by a noun
ending `-tion`, `-sion`, `-ment`, `-ance` or `-sis`. Write `analyse`,
not `conduct an analysis`. `provide` and `make` are NOT in the pattern:
both are ordinary verbs.

**Three abstractions where one concrete noun belongs**: three abstract
nouns in a list, as `clarity, rigor and craft`. Name the concrete thing.

**Requires approval per use.** `honest`, and every form of it, is banned
differently from the rest. The word is fine English; it is on the list
because it becomes a tic, where it flatters a sentence rather than says
anything the sentence did not already say: `the honest rendering`,
`the honest gap`, `keeps the copies honest`.

**The gate is absolute, and the lack of an inline exemption is the
point.** There is no `allow` comment and no suppression either gate would
honour, because an escape hatch that exists is an escape hatch that gets
used, and this is a word that is easy to reach for. A use the author
wants kept is approved by changing `reject.txt`: one line, in one file,
visible in review, which is where an approval belongs.

### What is not banned, and why

Several entries on the source lists are deliberately absent, because
they name things technical documentation describes. A gate that fires
on the subject matter is a gate people learn to switch off.

Four AiTells rules are absent for reasons of their own.
`VirtueHonest` is redundant: `honest(y|ly)?` above bans the stem, which
is broader than its phrase list. `StatusBracket`, which catches a
`[RED]`-style grade in prose, cannot be expressed here: both gates wrap
every line as `\b(?:...)\b`, and a pattern opening with `[` can never
satisfy that leading word boundary. `CodeToken`, which catches an
identifier written as prose, needs case to work, and `reject.txt` is
matched case-insensitively by design, so `[A-Z]` also matches lowercase
and it reports ordinary identifiers in quoted output. Its remaining
rules, `Passive`, `Adverb`, `EpigramContrast` and `NeverTag`, each fire
on a construction the house voice uses on purpose.

| Not banned | Because |
|---|---|
| `surface` | `the API surface` is how a reference describes an interface. The banned sense is the verb, for insights, and no rule can tell them apart. |
| `harness` | The noun is ordinary: `a test harness`, `an agent harness`. The banned sense is the verb. |
| `underscore` | A page about names writes about a literal `_`. The banned sense is "to emphasise". |
| `the point is` | `use it when the point is which failure this is` is a clause doing work, not the filler opener `The point is, ...`. |
| `guarantee` | A technical page states what a function or an algorithm guarantees, and the word is the precise one. |
| `above`, `below` | Ordinary cross-references inside a long reference page. |

The rule behind the list: ban the phrase that adds nothing, never the
word that names a thing.

**Matching spans a line wrap, in the local gate.** These pages wrap near
72 columns and most of the list is multi-word, so the prose gate joins
each paragraph before matching: `worth\nnoting` fails exactly as
`worth noting` does. Vale matches within a line and does not catch that
one, which is the second reason the local gate carries the list as well
as Vale rather than deferring to it.

**Patterns** (not mechanically checkable, enforced at review):

- Announcing structure before delivering it ("There are three things to
  understand").
- Restating the question before answering it.
- A closing one-liner that restates the thesis.
- Stacked short declaratives (four or more in a row).
- Superlative self-ranking ("the most important thing", "the part that
  matters most").
- A list of `**Bold term**: explanation` pairs, which is the single most
  recognisable machine-written list. Write sentences, or a table.

Avoid vague metaphors such as "the engine" for value, "shape" for an
argument, or "carry" for an idea. Use literal terms for the technical
subject: an API surface, a test harness, and a schema shape are valid.
Do not announce value with "real" or "useful"; state what the reader can do.
Do not invent claims about how much code agents produce or what teams know.
State the input, the check, the result, and any limit on that result.

**Punctuation rulings**:

- Do not use em dashes in prose. Use a comma, parentheses, a colon, or
  another sentence. Preserve punctuation in literal code and quoted output.
- In a link list, separate the link from its gloss with a full stop, not
  a dash: `- [Install the tool](how-to/install.md). Covers...`.
- A dash between a heading's number or label and its subject is a
  separator, not an aside, and neither spacing reads well once the dash
  closes up. A numbered section takes a full stop
  (`## 01. Service catalog`); a label takes a colon
  (`## Clause 1: hermeticity`).
- Exclamation marks: at most one per page, in tutorials only, on a
  genuine payoff.
- No emoji in documentation.
- Sentence-style capitalisation in headings (Google style).
- British spellings (`-ise`, `-isation`, `colour`). Google style is US
  English; this is one of the places the house voice wins, and
  `Google.Spelling` is switched off in `.vale.ini` for it. Actual
  misspellings are still caught: `Vale.Spelling` runs at error against
  `accept.txt`, the terms the project accepts by name.

## Code snippets: every one is tested

A fenced snippet in a tutorial, a how-to or a reference page is either
executed by the project's documentation tests or carries a visible,
reasoned skip. The directive vocabulary: an HTML comment on its own
line immediately before the fence:

```markdown
<!-- test: scenario deploy-gate -->
Opens a named scenario: one temporary directory, lasting until the next
scenario directive or the end of the page.

<!-- test: file config.json -->
The next fence is written to <scenario-dir>/config.json. Re-declaring
a name overwrites it (how "now change the file" recipes are modelled).
Name the file in the prose above the fence too, in a code span within
three lines.

<!-- test: run -->
The next sh fence is a transcript, executed in the scenario
directory. Lines starting "$ " are commands; following lines are the
expected output; "$ echo $?" pins the previous command's exit code; a
line holding only "..." matches any run of lines. Commands start with
the project's own command.

<!-- test: skip <reason> -->
Deliberately unexecuted, with a non-empty reason a reviewer can weigh.

<!-- test: quote <path> -->
The next fence is a run of lines of the repository file at <path>,
unchanged and in order: the page still says what the file says.

<!-- fmt: keep <reason> -->
The next fence keeps its spelling rather than the formatter's form
(below), with a non-empty reason a reviewer can weigh. It rides beside
a test directive, in either order.
```

The project's `AGENTS.md` names the harness that reads these. An
untagged fence (a diagram, quoted error text) makes no language claim
and is exempt.

**Every fence in a language the project formats is in the formatter's
form.** What the formatter writes is what the page shows, as Go's
documentation is `gofmt`-clean: write the example, run it through the
formatter, and paste what comes back. The exception is a fence whose
*spelling* is the lesson (a mistake shown on purpose, a document split
across files, the input a transcript formats), and it says so with
`fmt: keep` and a reason.

Two rules of taste:

- A doc transcript shows a moment: one or two commands, short output.
  Anything needing golden files or bad-input corpora is a runnable
  example in the test suite, and the doc links to it.
- Lift examples from the runnable examples wherever one fits. A reader
  who follows the link finds the same shape, alive, with its checks.

## Figures are generated

A picture of how a program works is a claim about it, and a claim drawn
by hand goes stale silently. **Every figure the program can draw is
generated**: written by a script the build runs, and held to the
program by a test. The project's `AGENTS.md` names the script.

- A page shows one with `![alt](figures/<name>.svg)`. The alt text says
  what the figure *shows*, in a sentence, not what it is called: a
  reader who cannot see it gets the same information.
- Box-drawing pictures are still fine where they are *output*: a
  transcript under a `test: run` directive is the program's own bytes,
  checked like any other transcript. What is banned is the hand-drawn
  one, which is a second source of truth for something the program
  already knows.

## Terminology

A project's terms of art are listed in its `AGENTS.md`, or in a
contributor page it links: each term, what it means, and the words it
is never swapped with. Four rules hold in every project:

- **The project's name is spelled one way**, in prose and in headings
  alike. `names` in `prose-gate.json` lists the spellings the gate
  refuses. An identifier that carries the name, such as an exported
  type or a Vale style directory, keeps its own case, because renaming
  it would break code rather than change prose.
- **Two words a reader takes as synonyms and the project does not are
  never swapped.** Module and package, say: where one is what an import
  names and the other is what a registry publishes, a sentence that
  uses one for the other is wrong however naturally it reads.
- **A term of art links to the page that defines it, on its first use
  in a page**; later uses on that page are plain. The same word in its
  ordinary English sense links nothing.
- **An error code is spelled as it renders.**

## Per-kind templates

**Tutorial section**: goal sentence → snippet → output → the one
observation → forward link. Every step's output shown, every snippet
executed.

**How-to guide**: title is the task in imperative or "-ing" form; one
sentence of situation; the recipe; one paragraph of what to watch
for; links (reference for the constructs, a runnable example for the
live version).

**Reference section**: definition, then behaviour, then edge cases,
then a pinned example. Every claim that has a test can name it.

**Explanation section**: the question, the answer, the argument, the
trade-off admitted. May quote history when the history is the
argument.

## Updating this guide

This guide is the same in every project that uses it. It is kept in
[agent-info](https://github.com/rjrodger/agent-info), where a rule
changes with its reasoning in the commit message, and a project takes
the change by copying the file again, never by editing its copy.

To ban a phrase, add the regular expression to
`.vale/styles/config/vocabularies/House/reject.txt` and summarise it in
the list above. Both gates pick it up from that one file; there is no
second list to update, and the prose gate's `guide-category` rule
checks that every `# --- … ---` category in `reject.txt` has a summary
here, so a whole category cannot be added to one and missed by the
other.

To change a Google rule's level, edit the project's `.vale.ini` and
write down what the rule produced on a clean run. "It was noisy" is
not a reason; "it reported 36 serial commas, every one a two-item list
after a comma clause" is. A rule demoted without that note reads later
as an oversight, and gets re-promoted by somebody repeating the work.
Run `node admin/scripts/vale-counts.cjs --write` afterwards: the note
is checked against a live run, so a number typed from memory fails.

To accept a word the spelling gate does not know, add it to the
project's `accept.txt` in the same directory, one stem at a time. Never
add a suffix pattern: `\w+ise` accepts `madeupise` too, and punches a
hole through the gate the file exists to make usable. Write a case pair
as one regular expression (`[Ss]tdin`), because two plain lines make
Vale enforce one spelling over the other.

# Custom grammars: writing an ABNF grammar for aless

aless can read a text format it has no built-in parser for when you hand
it an [ABNF](https://github.com/tabnas/abnf) grammar: `--grammar
NAME=FILE.abnf` (or `--grammar-expr NAME=ABNF`). This directory holds
one grammar per common Unix file, each with a realistic sample and the
JSON it parses to:

| grammar | sample | one record |
|---|---|---|
| `hosts.abnf` | `hosts.sample`, `hosts` | `{"address": "::1", "names": ["localhost", "ip6-localhost", "ip6-loopback"]}` |
| `crontab.abnf` (system, with the user field) | `crontab.sample`, `crontab` | `{"minute": "*/15", "hour": "*", "dom": "*", "month": "*", "dow": "*", "user": "root", "command": ["/usr/local/bin/check-disks", "--warn", "90"]}`, `{"when": "@reboot", "user": …, "command": […]}`, `{"env": "SHELL=/bin/sh"}` |
| `crontab-user.abnf` (`crontab -l`) | `crontab-user.sample` | the same without `user` |
| `passwd.abnf` | `passwd.sample` | `{"name", "password", "uid", "gid", "gecos", "home", "shell"}` |
| `group.abnf` | `group.sample` | `{"name", "password", "gid", "members": […]}` |
| `fstab.abnf` | `fstab.sample` | `{"spec", "file", "vfstype", "options": […], "freq", "passno"}` |
| `resolv.conf.abnf` | `resolv.conf.sample` | `{"nameserver": "10.0.0.53"}`, `{"search": [...]}`, `{"domain": …}`, `{"options": […]}`, `{"sortlist": […]}`, `{"comment": ["generated", "by", …]}` for a `;` line |
| `kv.abnf` (shell-style `KEY=value`, `export KEY=value`) | `kv.sample` | `{"key": "NAME", "value": "\"Ubuntu\""}` |

`NAME.expected.json` is the exact value (pretty-printed) that
`aless --grammar NAME=NAME.abnf --json` prints for `NAME.sample`. Every
value is an array of records, one per line of the file; `#` comment
lines and blank lines produce nothing (the one exception is
resolv.conf's `;` comment line, which the lexer does not drop and the
grammar keeps as a `comment` record). Field values are the source text,
always strings (`"uid": "0"`, not `0`).

```sh
aless --grammar hosts=tests/fixtures/grammars/hosts.abnf /etc/hosts
aless --grammar crontab=tests/fixtures/grammars/crontab.abnf --json /etc/crontab
crontab -l | aless --grammar crontab-user=tests/fixtures/grammars/crontab-user.abnf -k crontab-user
```

## How a grammar becomes a document

aless compiles the ABNF with `tabnas_abnf::abnf_convert` (the start rule
is the first production; `word_keywords` is on), adjusts the emitted
grammar's lexer options as described under "What aless sets" below,
installs it on a fresh engine, and parses the file. What the grammar
builds is the document: with `; @object` / `; @array` annotations a JSON
value, without them the `{rule, src, kids}` parse tree (note that `src`
there is the matched tokens run together, without the spaces between
them). Everything else in aless (`--json`, `--paths`, `--find`,
`--where`, `--render`, the viewer, watch mode) works on that value as on
any other format.

## Writing a line-oriented grammar

The whole of `hosts.abnf`:

```abnf
; /etc/hosts: an address and the host names it answers to, one per line.
; Comments start with # and blank lines are skipped, in every grammar here.
hosts   = *( entry %x0A / %x0A ) [ entry ]   ; @array
entry   = address names                      ; @object address names
address = word
names   = 1*word                             ; @array
word    = ( TX )
```

Each rule of that is one you will reuse.

- **Lines.** The engine's lexer skips spaces, tabs and comments between
  tokens, and would skip newlines too, but a grammar that spells the
  newline as the literal `%x0A` gets it as a token, because literals are
  matched before the newline skipper runs. Write the file as
  `*( entry %x0A / %x0A ) [ entry ]`: entries each followed by a
  newline, blank lines (`/ %x0A`) skipped, and the last line allowed to
  end without a newline when its shape has a fixed length (see "What
  does not work"). Use `%x0A`, not the core rule `LF` (see below). A
  CRLF file works too: the `\r` is treated as a space.
- **Comments.** `#` to the end of the line is dropped by the lexer, so
  a comment line looks like a blank line to the grammar and an inline
  comment ends the entry. There is nothing to write for comments.
- **Words.** `TX` is the built-in word token: a run of characters up to
  a space, tab, newline, `#` or one of the grammar's own literals. Under
  aless every word is a `TX`, including `127.0.0.1`, `::1`, `*/15`,
  `1-5`, `0,30`, `UUID=…`, `/dev/sda1`, `@reboot`, `true` and `0`
  (the JSON punctuation, the number and `true`/`false`/`null` lexers are
  off unless the grammar names `NR`, `ST` or `VL`). Wrap it in a group
  to make a field rule, `word = ( TX )`: a rule whose whole body is the
  bare token is turned into a token itself and no longer counts as a
  field, except in the leading position.
- **Fields.** `; @object a b` names one member per part of the rule that
  produces a value, in order: a rule reference, a group or a
  repetition. Literals (`":"`, `%x0A`, `"nameserver"`) are not members.
  The names are free (`; @object first second` works); they do not
  have to be rule names. A member that is a repetition or a group is
  the text it matched; an optional member (`password = [ word ]`) is
  `""` when absent.
- **Lists.** A member whose own rule is annotated is nested whole:
  `names = 1*word ; @array` gives one element per word. Comma lists:
  `members = [ word *( "," word ) ] ; @array`. The item of a list must
  be a rule (`1*word`); `1*( TX )` collects into a single element.
- **Separators.** A literal in the grammar is also a word delimiter:
  `passwd.abnf` names `":"`, so `root:x:0:0` splits into fields, and
  `group.abnf` names `","`, so `alice,bob` splits. `hosts.abnf` names
  neither, so `2001:db8::10` and `jan,jul` stay whole words. Choose the
  literals with that in mind: `kv.abnf` names `"="`, and so has to allow
  `"="` inside a value (`value = *( word / ST / "=" )`).
- **Spaces inside a field.** Naming `" "` makes every space a token
  (it is a literal, matched before the space skipper). `passwd.abnf`
  uses it for the gecos field, `gecos = *( word / " " )`, so
  `Alice Liddell,,,` comes through verbatim; the other fields have no
  spaces, so nothing else changes. Tabs would need `%x09` in the same
  way.
- **Keywords.** A quoted literal such as `"nameserver"` matches the
  whole word, case-insensitively, and does not match a prefix of a
  longer word (`nameservers` is a `TX`).
- **Alternatives.** Different line shapes are alternatives of the
  top-level repetition, each an annotated rule of its own:
  `*( timed %x0A / special %x0A / setting %x0A / %x0A ) [ timed /
  special / setting ]`. An intermediate `entry = timed / special` is
  refused (the leading references would be folded into it). Two shapes
  may share a prefix (`setting` is one word, `timed` starts with the
  same word), the compiler factors it out; but the choice must be
  decidable within about four tokens of where they diverge.
- **Quoted strings.** Name `ST` to get quoted strings as one token,
  quotes included: `kv.abnf` does, so `NAME="Ubuntu 24.04"` gives
  `"\"Ubuntu 24.04\""`. Where a grammar does not name `ST`, a quote is
  an ordinary character and `"Stand-up in 30 minutes"` is four words
  (`crontab-user`); that is deliberate, since an unbalanced quote in a
  command would otherwise fail the whole parse.

## What does not work

Measured against tabnas-abnf 0.4.16 / tabnas-bnf 0.1.22; each item is a
minimal grammar and what happened.

- **Character-level rules beside `TX`.** `rest = 1*( %x21-7E )` compiles
  to an eager per-character token that fires at every position, so
  every word in the file is lexed one character at a time and `word =
  ( TX )` never matches (`foo bar` fails on `f`). Use `TX`, `NR`, `ST`,
  `VL` and literals only, or write the whole grammar at character level
  (as RFC grammars do) and never `TX`. There is therefore no
  "rest of the line" field: `crontab.abnf` gives the command as a list
  of words instead.
- **`LF`, `CRLF`, `WSP` in an annotated rule.** The core rules are rule
  references, so `line = a b LF ; @object a b` is refused ("names 2
  members but has 3 parts"). Spell them as literals: `%x0A`, `%x0D
  %x0A`, `" "`.
- **A single-literal rule in an annotated rule.** `sep = ":"` with
  `line = a sep b ; @object a b` is refused the same way, although the
  compiler does turn `sep` into a token when no rule is annotated
  (`options.fixed.token.#sep`). Write the literal inline.
- **`*" "` as spacing inside an annotated rule.** A repetition counts as
  a member even when it is made of literals, so
  `entry = key *" " "=" *" " value ; @object key value` is refused
  (5 parts). This is why `kv.abnf` accepts `KEY=value` only, with no
  spaces around the `=`; a value with spaces must be quoted
  (`KEY=a b` gives `"ab"`).
- **A bare token as a non-leading field.** `line = a b %x0A ; @object a
  b` with `a = TX`, `b = TX` is refused ("names 2 members but builds
  1"): `b` becomes a token. `( TX )` always works.
- **`entry = timed / special` over annotated rules** is refused; put
  the alternatives in the top-level repetition (above).
- **Empty input** parses to `null`, not `[]` (the engine's
  `lex.empty`); a file with only comments or blank lines gives `[]`.
- **Two shapes told apart only by word count.** `MAILTO = root` (three
  words) is neither a one-word setting nor a seven-word entry in
  `crontab.abnf` and fails the parse; `MAILTO=root` is fine.
- **A last line of open-ended shape with no newline after it.**
  `1.1.1.1 a b` as the whole of a hosts file, or `search a b c` as the
  last line of a resolv.conf, fails with "unexpected end of input": once
  `1*word` has run to the end of the input the trailing `[ entry ]`
  cannot be told from `entry %x0A`, since the compiler decides between
  two shapes within a few tokens of where they diverge. A last line of
  fixed shape (`nameserver 1.1.1.1`, a passwd entry) ends without a
  newline fine, and every line ending in a newline is fine.
- **Files past about 500,000 lines.** The compiler writes a repetition
  as a rule that calls itself once per item, so the engine keeps a rule
  open for every item matched so far: two per line for the grammars
  here. aless allows a grammar from the command line 1,000,000 open
  rules (the built-in grammars stop at 3,000, which these grammars would
  reach at 1,500 lines) and measures nesting on the value instead, so
  the limit is some 500,000 lines, 30 MB of `hosts`, at about 10 KB of
  memory a line (300,000 lines: 24 s and 2.8 GB in a release build).
  Past it the parse fails as `too_deep`, naming the open rules. This is
  tabnas-bnf's `*`/`1*` desugaring (`H = inner H / ε`), which only the
  `X = prefix [ sep X ]` shape escapes as a same-depth repeat.

  That cap is a workaround, and the rule it works around is the fleet's:
  a repetition is replacement, never a push chain. An alternate in the
  engine either pushes a child rule (`p`), a new frame for something the
  tree must nest, or replaces the current rule (`r`), the same frame
  re-entered for the next item of a sequence. `*entry` is a sequence, so
  it is meant to compile to a replace loop, the loop `r` and the item
  `p` where it nests, and rule depth (the engine's `d`) is then bounded
  by the grammar's nesting and never by the file's length: a hosts file
  of any length costs the depth of one line. tabnas-bnf's `H = inner H /
  ε` is a push chain instead, which is why aless lets a grammar from the
  command line open 1,000,000 rules; once aless pins a bnf and abnf that
  compile the star as `r`, that cap goes back to the shared 3,000 and
  this item goes away. Write `*entry` and leave the loop to the compiler
  rather than spelling it as a rule that calls itself, which is the same
  chain by hand. Rule depth over a repetition is constant; a test that
  repeats an item ten thousand times and asserts the maximum `d` stays
  what a single item needs is the proof.

## Simplifications per format

- `hosts`: none.
- `crontab`, `crontab-user`: the command is a list of whitespace-separated
  words (see above); a `#` inside a command starts a comment, as it
  does in hosts; environment lines are one word, `NAME=value`, kept as
  `{"env": "NAME=value"}`; `@reboot` and the other `@` shortcuts are
  the `when` field.
- `passwd`: any field may be empty; gecos keeps its spaces and commas.
- `group`: `members` is `[]` for an empty list.
- `fstab`: `vfstype` may be a comma list (`udf,iso9660`) and is kept as
  text; `options` is a list; `freq` and `passno` are `""` when omitted.
- `resolv.conf`: each keyword is its own record shape; unknown keywords
  fail the parse. A `;` comment line, which resolv.conf(5) allows
  beside `#`, is kept as `{"comment": [words]}` (`{"comment": []}` for a
  bare `;`), since the lexer drops `#` comments only and a grammar
  cannot match a line and build nothing; a `;` anywhere but the first
  column fails the parse.
- `kv`: `KEY=value` with no spaces around `=`; the value keeps its
  quotes; `#` starts a comment unless inside quotes. `export KEY=value`
  is accepted and the `export` dropped (a second line shape, since an
  optional literal would count as a member); as `export` is then a
  keyword, a key named `export` itself fails the parse.

## What aless sets (findings for the implementation)

The compiled grammar carries only the tokens the grammar names; the
engine's defaults for everything else are JSON's, and they break plain
text. Before `spec.install(&mut parser)`, aless merges this into
`spec.options` (paths as in the engine's `options` block):

| path | value | why |
|---|---|---|
| `fixed.token.#OB`, `#CB`, `#OS`, `#CS`, `#CL`, `#CA` | `null` each | `{ } [ ] : ,` are otherwise word delimiters: `::1` becomes three tokens, `0,30` three, `root:x:0:0` seven. Required. |
| `comment.def.slash`, `comment.def.multi` | `null` each | `//` and `/* */` are otherwise comments: `//server/share`, `http://…` lose the rest of the line. `comment.def.hash` (`#` to end of line) stays. Required. |
| `number.lex`, `string.lex`, `value.lex` | `false` unless the grammar names `NR`, `ST`, `VL` respectively (the token name appears in an alternate's `s`; a substring check over the serialized `rule` table is enough) | A word class the grammar never names is folded into `TX`: `0` and `17` are words, quotes are characters, `true` is a word. Six of the eight fixtures fail without this (`#NR` where `TX` is expected). Required. |
| `space.chars` | `" \t\r"` | A CR before the LF is trivia, so CRLF files parse; without it the line skipper takes `\r\n` together and the grammar never sees the newline. Recommended. |

Conversion options (`AbnfConvertOptions`): `start` = the first
production, `word_keywords: true` (a keyword literal matches whole
words only; without it `nameservers 5.6.7.8` fails against
`"nameserver"`), everything else default. Nothing about newlines needs
setting: `tokenSet.IGNORE` keeps `#SP #LN #CM`, and a grammar that
names `%x0A` gets its newlines as fixed tokens before the `#LN` skipper
runs. `lex.empty` stays as emitted (`true` for these grammars), which is
why an empty file is `null`.

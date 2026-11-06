# Prompt log

## 2026-11-03 17:09 UTC

Agent: Claude Code, session https://claude.ai/code/session_01D85fCHxLMUJrquF8wFT2Dv

```
the removed features are ... removed. there should be no mention of them. refer to https://github.com/rjrodger/agent-info - set up the aless project following this approach
```

## 2026-11-03, about 17:20 UTC

Agent: Claude Code, session https://claude.ai/code/session_01D85fCHxLMUJrquF8wFT2Dv

The answer to a question the agent asked: what should happen to
`CHANGELOG.md`'s released entries, which describe the removed features.

```
Keep as history
```

## 2026-11-03 17:48 UTC

Agent: Claude Code, session https://claude.ai/code/session_01D85fCHxLMUJrquF8wFT2Dv

```
fix the findings, here and in agent-info (checkout to rjrodger/agent-info locally); then ask me about each ADR in order to refine it or remove it
```

## 2026-11-03, about 18:10 UTC

Agent: Claude Code, session https://claude.ai/code/session_01D85fCHxLMUJrquF8wFT2Dv

The answers to the agent's questions on each entry of `ADR.md`, asked as
the prompt of 17:48 UTC directs.

```
ADR-001: Keep as written (Recommended)
ADR-002: Keep as written (Recommended)
ADR-003: Keep as written (Recommended)
ADR-004: Keep as written
ADR-005: Keep as written (Recommended)
ADR-006: Keep as written (Recommended)
ADR-007: Remove
```

## 2026-11-03 17:57 UTC

Agent: Claude Code, session https://claude.ai/code/session_01D85fCHxLMUJrquF8wFT2Dv

```
does the documentation pay sufficient tribute to jless, which is the inspiration for this tool
```

## 2026-11-03 18:33 UTC

Agent: Claude Code, session https://claude.ai/code/session_01D85fCHxLMUJrquF8wFT2Dv

The answer to the agent's offer to add a tribute to jless to aless#106.

```
yes
```

## 2026-11-04 09:38 UTC

Agent: Claude Code, session https://claude.ai/code/session_01D85fCHxLMUJrquF8wFT2Dv

```
Merge
```

## 2026-11-04 09:50 UTC

Agent: Claude Code, session https://claude.ai/code/session_01D85fCHxLMUJrquF8wFT2Dv

The answer to the agent's offer to push agent-info's comment-gate fix,
the copy aless carries, and open its pull request.

```
push and merge
```

## 2026-11-04 10:29 UTC

Agent: Claude Code, session https://claude.ai/code/session_01D85fCHxLMUJrquF8wFT2Dv

The answer to the agent's offer to bring aless's copies up to date with
agent-info, which now calls itself a template.

```
bring up to date
```

## 2026-11-04 11:08 UTC

Agent: Claude Code, session https://claude.ai/code/session_01D85fCHxLMUJrquF8wFT2Dv

```
merge; then completely review and rewrite the documentation and website
```

## 2026-11-04 11:47 UTC

Agent: Claude Code, session https://claude.ai/code/session_01D85fCHxLMUJrquF8wFT2Dv

```
pr, merge, deploy site when green
```

## 2026-11-04 12:14 UTC

Agent: Claude Code, session https://claude.ai/code/session_01D85fCHxLMUJrquF8wFT2Dv

```
is the style guide applied? does the text passs?
```

## 2026-11-04 12:18 UTC

Agent: Claude Code, session https://claude.ai/code/session_01D85fCHxLMUJrquF8wFT2Dv

The answer to the agent's offer to close the style guide's gaps it had
listed: the untested fences, the unlinked terms, the code with no colon
sentence before it, and the em dashes in aless's own output.

```
yes fix it all
```

## 2026-11-04 13:24 UTC

Agent: Claude Code, session https://claude.ai/code/session_01D85fCHxLMUJrquF8wFT2Dv

The answer to the agent's question whether to merge #109, whose CI had
passed, and so deploy the site.

```
merge and deploy
```

## 2026-11-04 17:06 UTC

Agent: Claude Code, session https://claude.ai/code/session_01D85fCHxLMUJrquF8wFT2Dv

```
add the following as a new section "Why" (top level nav), with a single page containing the following text (add some aless screenshots where they are mentioned as examples):

When I started working hands-on with [OpenAPI specifications](https://spec.openapis.org/oas/), it became
clear very quickly that using an editor to read large volumes of
deeply nested JSON sucks, a very great deal. So I went looking for
something to help, and came across [jless.io](https://jless.io/).  This is an awesome
terminal app for navigating nasty JSON documents. I am forever
indebted to [Paul Martinez](https://github.com/PaulJuliusMartinez) for
writing this tool - it made my early work on our [SDK
generator](https://voxgig.com/sdk) so much easier.

And while `jless` is very cool it did have some gaps. It couldn't
reload files that had changed, keeping your place in the JSON
structure, which I really needed. And it only loaded one file at a
time, and I was working with many OpenAPI files. But unfortunately it
was written in [Rust](https://rust-lang.org/), and I didn't know Rust, and back in the pre-AI
days that was kind of a huge blocker. I did consider making it a
motivation to actually take the time to learn Rust, but, you know, life.

So I struggled on. jless was still a big win.

But of course once we got models that could actually write decent
code, the obvious idea came to me - I'll just add my features and
submit a PR! Happy days. Except by that stage my requirements had
grown - I also needed to handle many other file formats, like CSV,
TOML, INI, and even XML. So I got side-tracked extending another
side-project - an easy going JSON parser that supported comments, and
trailing commas, the usual sort of thing.

During COVID, with a SaaS startup in the events space now on the
rocks, I was looking for some distraction and decided to refactor this
little easy-going JSON parser of mine (which I had called
[`jsonic`](https://github.com/jsonicjs/jsonic)). It used a generated parser and was pretty slow. I read some
blog posts about how all production-grade language parsers are pretty
much hand-crafted recursive descent, and thought, how hard could it
be!

Five years later, after many many fun experiments, and production use,
I ended up with a runtime parser system: [tabnas.dev](https://tabnas.dev/). This is a quirky
approach to parsing that is partly recursive descent, partly a state
machine, and mostly empirical let's see what happens hacking. It works,
and it's pretty fast, and I use it for real projects. It has the nice
property that grammars are dynamic - you don't generate code, the
grammar is executed. A fancy regular expression engine, but revved up
to parse anything into a proper abstract syntax tree.

Now with that in hand, I could return to the original project and
create my own version of jless that solved the itches I needed to
scratch:
* a watch mode to reload files
* multiple tabs to hold multiple files
* load any data format into a tree structure

Now I still don't know Rust. I mean I like the idea, but there are
only so many hours in the day. But now we have AI, so now I do get to
go to the candy store. Combining [jless.io](https://jless.io/), and [tabnas.dev](https://tabnas.dev/), and my
requirements, I built (caused to be built?), [`aless`](https://aless.tabnas.dev/) - the [`less`](https://www.greenwoodsoftware.com/less/) for
any format. Here's a screenshot or two, and the site is
[aless.tabnas.dev](https://aless.tabnas.dev/).  Now this is also a nice stress test for the tabnas
parsing engine, and I was very pleased about that too.

So if you need to review lots of data structure files on the command
line, and need a terminal app to do so, this is for you. I can't help
but be struck by the irony however that the need for such a thing is
much less now - I mostly get agents to look at large JSON documents
now. Still, our puny human brains need help sometimes, and this lets
me review documents when I need to.

It is also really nice and satisfying to see how easy it is to now
bring together open source components to create what is essentially
personal software. Obviously since I write a lot of Open Source anyway
it is easy for me to release this as a self-contained installable
command line tool, which others might find useful too.

But I think the key thing out of this little project is that the basic
physics of software development remain the same. Software should be
built from components glued together. Components should have
well-defined interfaces. Aim to reduce cognitive load - this is useful
to both humans and machines.

Yes, the entire thing could be built from scratch, using a textual
specification, and be a mire of over-engineered AI slop, and still be
perfectly usable. But the way I actually built it, with AI as a coding
companion, using components was deeply satisfying because I still
understand the essence of the code, while being able to ignore the
accidental complexity. Components give you another layer of
abstraction with fewer things to think about, and that allows your
human mind to stay in the game.
```

## 2026-11-04 17:09 UTC

Agent: Claude Code, session https://claude.ai/code/session_01D85fCHxLMUJrquF8wFT2Dv

Two messages sent while the agent worked on the prompt above, logged
when the agent read them.

```
also mark the why section as human written, and not subject to the style guide
```

```
publish to the live site
```

## 2026-11-04 17:11 UTC

Agent: Claude Code, session https://claude.ai/code/session_01D85fCHxLMUJrquF8wFT2Dv

Sent while the agent worked on the Why page, logged when the agent read
it.

```
is aless hiding . prefix files and folders? this is a developer tool - show them
```

## 2026-11-04 17:22 UTC

Agent: Claude Code, session https://claude.ai/code/session_01D85fCHxLMUJrquF8wFT2Dv

```
do not literally put human written on the actual page. I meant keep a note for yourself so that the style-guide is not applied to this text. also, remove the mini eyebrow headings entirely from the site
## 2026-11-04 17:15 UTC

Agent: Claude Code, session https://claude.ai/code/session_01D85fCHxLMUJrquF8wFT2Dv

A follow-up to the prompt about dot-files, sent while the agent worked.

```
so --hidden is no longer needed?
```

## 2026-11-04 17:34 UTC

Agent: Claude Code, session https://claude.ai/code/session_01D85fCHxLMUJrquF8wFT2Dv

```
the writing rules in rjrodger/agent-info have been updated - copy the updates into this repo. then rewrite the documentation and site text, carefully following the updated rules. use adversarial review to check and correct your work
```

## 2026-11-04 17:57 UTC

Agent: Claude Code, session https://claude.ai/code/session_01D85fCHxLMUJrquF8wFT2Dv

The answer to the agent's question whether to merge #111 (dot-files
shown, --hidden removed) and #112 (no eyebrow labels), both green.

```
merge both
```

## 2026-11-04 20:46 UTC

Agent: Claude Code, session https://claude.ai/code/session_01D85fCHxLMUJrquF8wFT2Dv

Sent after the rewrite workflow stopped short, its usage credits spent,
with every page rewritten and reviewed once but later reviews, some
fixes and the consistency pass not run.

```
Continue
```

## 2026-11-04 22:58 UTC

Agent: Claude Code, session https://claude.ai/code/session_01D85fCHxLMUJrquF8wFT2Dv

The answer to the agent's offer to merge #113 (the writing rules, with
the documentation and the binary's text rewritten to them), whose CI had
passed, and so deploy the site.

```
Merge and deploy
```

## 2026-11-05 10:56 UTC

Agent: Claude Code, session https://claude.ai/code/session_01D85fCHxLMUJrquF8wFT2Dv

```
on the site, the favicon should reflect the actual logo. no direct links to index.html files - use the domain and folder path format
```

## 2026-11-05 11:34 UTC

Agent: Claude Code, session https://claude.ai/code/session_01D85fCHxLMUJrquF8wFT2Dv

Sent after asking how to view local changes to the site, and whether the
preview directory is ignored by git.

```
add a scripts top level folder, and add script to build the site, and then view it
```

## 2026-11-05 12:51 UTC

Agent: Claude Code, session https://claude.ai/code/session_01D85fCHxLMUJrquF8wFT2Dv

About the jless link in the owner's own uncommitted edit to docs/index.md.

```
fix the broken link
```

## 2026-11-05 12:54 UTC

Agent: Claude Code, session https://claude.ai/code/session_01D85fCHxLMUJrquF8wFT2Dv

About the home page's new intro in the owner's uncommitted edit, after
the agent reported that its lede held a paragraph browsers close it on.

```
fix the intro. what does the prose gate say?
```

## 2026-11-05 13:00 UTC

Agent: Claude Code, session https://claude.ai/code/session_01D85fCHxLMUJrquF8wFT2Dv

Sent after the owner reworded the home page's intro to pass the prose
gate; the go-ahead to push it with #114, merge, and deploy the site.

```
reworded, push and publish
```

## 2026-11-05 13:17 UTC

Agent: Claude Code, session https://claude.ai/code/session_01D85fCHxLMUJrquF8wFT2Dv

```
on the on the https://aless.tabnas.dev/how-to/install.html page, some of the code boxes are too wide and fading out on the right. don't do this, just user overflow scroll. Also for code boxes that show terminal cmds, which the user will copy, add a copy icon (the two overlapping squares convention)
```

## 2026-11-05 15:04 UTC

Agent: Claude Code, session https://claude.ai/code/session_01D85fCHxLMUJrquF8wFT2Dv

The go-ahead to merge #115 (code boxes scroll without a fade; a copy
button on each box of commands), whose CI had passed, and so deploy it.

```
merge 115
```

## 2026-11-05 16:30 UTC

Agent: Claude Code, session https://claude.ai/code/session_0167vhgQg2UPbMitgn5uV64t

```
do a full release
```

## 2026-11-05 16:37 UTC

Agent: Claude Code, session https://claude.ai/code/session_0167vhgQg2UPbMitgn5uV64t

Sent while the agent waited on the CI of #116 (aless 0.7.0, the version
bump of the release asked for above).

```
merge 116 when green
```

## 2026-11-05 17:31 UTC

Agent: Claude Code, session https://claude.ai/code/session_0167vhgQg2UPbMitgn5uV64t

A question, asked after the 0.7.0 release.

```
when a file reloads, are the changes highlighted?
```

## 2026-11-05 17:34 UTC

Agent: Claude Code, session https://claude.ai/code/session_0167vhgQg2UPbMitgn5uV64t

Sent after the agent answered that a reload does not highlight what
changed.

```
note this as a feature request, posting it to github issues for the repo, and crediting https://x.com/KyrelleBuckley
```

## 2026-11-06 10:54 UTC

Agent: Claude Code, session https://claude.ai/code/session_0167vhgQg2UPbMitgn5uV64t

```
How does aless deal with $ref in yaml
```

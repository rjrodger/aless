---
title: Why aless
description: Why aless exists, from reading OpenAPI files in jless, by way of jsonic and the tabnas parsing engine, to a viewer built with AI from components.
author: Richard Rodger
---
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
[aless.tabnas.dev](https://aless.tabnas.dev/).

<!-- screen: aless bookshelf-openapi.yaml books.json hosts.abnf
keys: j j h c J J J l / listBooks Enter
edit: bookshelf-openapi.yaml: The books on a shelf => Every book on a shelf, in order
size: 80x16 -->
```screen
 1:bookshelf-openapi.yaml*   2:books.json*   3:hosts.abnf*
  │           $ref: "#/components/responses/NotFound"
  └─▽ "/api/shelf/{shelf_id}/book": {
    │ ▽ parameters: [
    │   ▽ {
    │       $ref: "#/components/parameters/ShelfId"
    └─▽ get: {
      │ ▽ tags: [
      │     "shelf"
      │   operationId: "listBooks"
      └─~ summary: "Every book on a shelf, in order"
        ▽ parameters: [
          ▽ {
 14:00:05 ○──────────────────────────────────────────────────● 14:02:11  2 of 2
…l .paths["/api/shelf/{shelf_id}/book"].get.operationId yaml · watching · 104:7
Reloaded bookshelf-openapi.yaml: 1 changed
```

<!-- screen: printf 'title,author,year\nA Book of Examples,A. N. Author,1999\nShelves and How to Fill Them,B. Bookworm,2012\nThe Index,C. Cataloguer,2021\n' | aless -k csv
size: 80x15 -->
```screen
▼ [
  ▽ {
      title: "A Book of Examples"
      author: "A. N. Author"
      year: "1999"
  ▽ {
      title: "Shelves and How to Fill Them"
      author: "B. Bookworm"
      year: "2012"
  ▽ {
      title: "The Index"
      author: "C. Cataloguer"
      year: "2021"
 (stdin) .                                                            csv · 2:1
```

Now this is also a nice stress test for the tabnas
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

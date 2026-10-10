---
title: Give an agent aless
description: Install the Agent Skill that teaches an AI agent to read, query, check and convert files with aless's command line.
order: 13
---
aless's command line was made to be driven by programs, and an agent is one more. It carries an [Agent Skill](https://agentskills.io), a Markdown file of instructions an agent loads when a task needs it, which teaches the outputs, the errors and the exit statuses. aless prints the skill itself, so its version always matches the binary's.

## Install the skill

For Claude Code, put it in the skills directory:

```sh
mkdir -p ~/.claude/skills/aless
aless --generate skill > ~/.claude/skills/aless/SKILL.md
```

For another agent, write it into the directory that agent loads skills from. The file starts with the name and the description an agent matches against its task:

```console
$ aless --generate skill | head -4
---
name: aless
description: Read, query, validate and export structured files with the aless command-line tool, without its terminal viewer. Formats are JSON, JSON Lines, JSON5, JSONC, jsonic, YAML, TOML, INI, CSV, TSV, XML, ZON, Markdown, RSS/Atom, CSS, Protocol Buffers .proto files, PGN chess games, arithmetic expressions and semantic versions, plus any line-oriented text format described by an ABNF grammar given on the command line (/etc/hosts, crontabs, passwd, fstab). Use it to outline a large or unfamiliar file, to print the value at a path as JSON, and to find which path and source line a key or value is at. It also maps a line:col from a linter, test or stack trace to the structural path it points into. It converts any of those formats to JSON for jq, exports the records in one as CSV (streamed, so JSON Lines and CSV of any size), writes any of them as any format that has a render (YAML, TOML, INI, XML, ZON, JSON Lines, a Markdown table, JSON, CSV, an Atom feed, an expression; CSS, .proto, PGN and a version from documents of their own kind), runs a program in the alchemy streaming language over one (select, project, reshape and render on the way through), and checks that files parse, reporting the parser's exact error position and hint.
---
```

Write it again after you upgrade aless.

## Without the skill

An agent with no skill loaded can still use aless by following five rules, which `aless --help` opens with:

1. Pass an output option (`--json`, `--paths`, `--find`, `--where`, `--check`, `--render` or `--alchemy`), never `--panes`, which opens the viewer.
2. Name the file as an argument; standard input is read as JSON unless `-k FORMAT` says otherwise.
3. Check the exit status before reading standard output: only 0 means it holds the answer (`--check`'s report comes with 1 too).
4. Quote a path for the shell: `--path '.items[0]'`.
5. Start small on a big or unknown file: `--paths --depth 1`, then `--path` into it.

aless never waits for a key: without a terminal, a run that would open the viewer fails at once with the status 2. The [command line reference](/reference/command-line.html) is the whole contract, and [why the command line answers in JSON](/explanation/command-line.html) says why it is shaped as it is.

This site is also written for agents to read: [llms.txt](/llms.txt) lists every page, and each page has a Markdown version at its address with `.md` in place of `.html`.

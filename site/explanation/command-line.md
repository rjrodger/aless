---
title: Why the command line answers in JSON
description: The design of aless's interface for scripts and agents, and why it never waits, prints one answer, and keeps its output shapes stable.
order: 4
---
aless has two interfaces to one tree. The viewer is for a person at a terminal. The command line is for a program: a shell script, a CI job, or an AI agent working in a terminal. A program cannot look at a screen, cannot press keys, and cannot ask what an ambiguous message meant, so the command line is designed around what a program can do, which is read a status and parse JSON.

## One answer, on standard output

Every run reads its input, prints one answer on standard output, and exits 0, or prints one error object on standard error and exits with a status that says what kind of failure it was. Nothing else is written to standard output, so a caller never has to separate the answer from chatter. The answer is JSON whatever the input format was, because JSON is what every language and tool can already read, and because the tree aless reads every format into is JSON's model.

The error is JSON for the same reason. A message meant for people would need parsing by a program, and would change wording between releases. aless's errors carry their facts as fields instead: the kind, the file, the line and column, the parser's code and hint, and for a path that names nothing, the nearest node it did reach and that node's keys, which is what a caller needs to try again.

## Never waiting

A program that runs aless and waits for its answer cannot press a key, so aless never waits for one. Without a terminal it does not start the viewer, and a run that would need the viewer says so at once, before it reads any input, with a usage error and the status 2. That is the reason a pipe or a file on standard output is enough to choose the command line: an agent's tool call has no terminal, and gets JSON without asking.

## Paths a caller can give back

Every output names nodes by their paths, in jq's syntax, and `--path` takes the same syntax, so an answer can go straight back in as the next question. That is the loop an agent runs on a large file it does not know: an outline one level deep, then a path into the part it wants, then the value. Nothing in the loop needs the whole file in the answer, which keeps it within what an agent can read.

## A contract

The shapes of the outputs and the errors, and the meanings of the exit statuses, are a contract: a release may add a field, and never renames, removes or changes the meaning of one. Scripts written against one release keep working on the next. The contract is written out in the [reference](/reference/command-line.html), in the README and in the Agent Skill aless prints, and aless's tests hold its outputs to it. [Give an agent aless](/how-to/agents.html) shows how to hand it to an agent.

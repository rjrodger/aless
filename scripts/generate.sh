#!/usr/bin/env bash
# generate.sh: the files aless writes from its own reference (src/cli.rs),
# committed so that a release archive, a package or a source checkout has
# them without running aless: the man page and the shells' completions.
#
#   scripts/generate.sh
#
# tests/agent.rs fails when a committed file differs from what the built
# binary writes, and names this script. Run it after changing an option
# or the reference, and after a version bump (the man page names the
# version, and the date the changelog gives it).
set -euo pipefail

cd "$(dirname "$0")/.."

echo "generate: building aless" >&2
cargo build --locked --quiet
bin=target/debug/aless

mkdir -p man completions
for pair in \
  "man man/aless.1" \
  "complete-bash completions/aless.bash" \
  "complete-zsh completions/_aless" \
  "complete-fish completions/aless.fish" \
  "complete-powershell completions/_aless.ps1"; do
  set -- $pair
  "$bin" --generate "$1" > "$2"
  echo "generate: wrote $2" >&2
done

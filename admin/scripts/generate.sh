#!/usr/bin/env bash
# The files aless writes from its own reference (src/cli.rs), committed so a
# release archive or a package has them: the man page and the completions.
# tests/binary.rs fails until this runs after an option, the reference or
# the version changes.
set -euo pipefail

cd "$(dirname "$0")/../.."

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

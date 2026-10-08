#!/usr/bin/env bash
# yaml-fixtures.sh: the tabnas/yaml checkout tests/yaml_render.rs reads.
#
# The published tabnas-yaml crate ships only its rs/ directory, and the
# test reads the repository's test/spec and its vendored YAML Test Suite.
# This clones tabnas/yaml at the tag of the tabnas-yaml version Cargo.lock
# pins, under target/yaml-fixtures/<version> (a second run reuses it, and
# `cargo clean` removes it), and prints the line that names it:
#
#   eval "$(scripts/yaml-fixtures.sh)"    # then cargo test --locked
#
# On GitHub Actions it also appends TABNAS_YAML_DIR to $GITHUB_ENV, so the
# later steps of the job see it. The path is relative to the repository
# root, which the test resolves it against, so the same line works on
# Windows. Progress goes to standard error; standard output is only the
# export line.
set -euo pipefail

cd "$(dirname "$0")/.."

version=$(awk '$0 == "name = \"tabnas-yaml\"" { getline; gsub(/^version = "|"$/, ""); print; exit }' Cargo.lock)
if [ -z "$version" ]; then
  echo "yaml-fixtures: Cargo.lock pins no tabnas-yaml" >&2
  exit 1
fi

dir="target/yaml-fixtures/$version"
if [ ! -f "$dir/rs/Cargo.toml" ]; then
  rm -rf "$dir"
  mkdir -p "$(dirname "$dir")"
  # The release workflow writes both tags on the commit it published; the
  # crate is published from the go/v one.
  for tag in "go/v$version" "ts/v$version"; do
    echo "yaml-fixtures: cloning tabnas/yaml at $tag into $dir" >&2
    if git -c advice.detachedHead=false clone --quiet --depth 1 --branch "$tag" https://github.com/tabnas/yaml "$dir" >&2; then
      break
    fi
  done
  if [ ! -f "$dir/rs/Cargo.toml" ]; then
    echo "yaml-fixtures: tabnas/yaml has no go/v$version or ts/v$version tag" >&2
    exit 1
  fi
fi

if [ -n "${GITHUB_ENV:-}" ]; then
  echo "TABNAS_YAML_DIR=$dir" >>"$GITHUB_ENV"
fi
echo "export TABNAS_YAML_DIR=$dir"

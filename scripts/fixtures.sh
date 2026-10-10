#!/usr/bin/env bash
# fixtures.sh: the grammar repositories' own fixtures, which aless's tests
# read and the published crates do not ship.
#
# tests/translate_matrix.rs writes every document of every format's corpus
# (each repository's test/spec/*.tsv, JSONTestSuite's documents in jsonc's)
# into every format and reads it back; tests/yaml_render.rs reads
# tabnas/yaml's spec and its vendored YAML Test Suite. The crates ship only
# their rs/ directories, so this clones each grammar repository whose crate
# carries translation parts (tabnas/chess is PGN's) at the tag of the
# version Cargo.lock pins, under target/fixtures/<repository>/<version> (a
# second run reuses a checkout, and `cargo clean` removes them), and prints
# the lines that name them:
#
#   eval "$(scripts/fixtures.sh)"    # then cargo test --locked
#
# On GitHub Actions it also appends both to $GITHUB_ENV, so the later steps
# of the job see them. The paths are relative to the repository root, which
# the tests resolve them against, so the same lines work on Windows.
# Progress goes to standard error; standard output is only the export
# lines.
set -euo pipefail

cd "$(dirname "$0")/.."

REPOSITORIES="chess css csv expr feed ini json json5 jsonc jsonic jsonl markdown proto semver toml xml yaml zon"

# The version Cargo.lock pins for a crate.
locked() {
  awk -v name="$1" '$0 == "name = \"" name "\"" { getline; gsub(/^version = "|"$/, ""); print; exit }' Cargo.lock
}

total=$(wc -w <<<"$REPOSITORIES")
n=0
for repo in $REPOSITORIES; do
  n=$((n + 1))
  version=$(locked "tabnas-$repo")
  if [ -z "$version" ]; then
    echo "fixtures: Cargo.lock pins no tabnas-$repo" >&2
    exit 1
  fi
  dir="target/fixtures/$repo/$version"
  if [ -f "$dir/rs/Cargo.toml" ] && [ -d "$dir/test/spec" ]; then
    echo "fixtures: $repo $version, present ($n of $total)" >&2
    continue
  fi
  rm -rf "$dir"
  mkdir -p "$(dirname "$dir")"
  # The release workflow writes both tags on the commit it published; the
  # crate is published from the go/v one.
  for tag in "go/v$version" "ts/v$version"; do
    echo "fixtures: cloning tabnas/$repo at $tag ($n of $total)" >&2
    if git -c advice.detachedHead=false clone --quiet --depth 1 --branch "$tag" "https://github.com/tabnas/$repo" "$dir" >&2; then
      break
    fi
  done
  if [ ! -d "$dir/test/spec" ]; then
    echo "fixtures: tabnas/$repo has no go/v$version or ts/v$version tag with test/spec" >&2
    exit 1
  fi
done

yaml="target/fixtures/yaml/$(locked tabnas-yaml)"
if [ -n "${GITHUB_ENV:-}" ]; then
  echo "TABNAS_FIXTURES_DIR=target/fixtures" >>"$GITHUB_ENV"
  echo "TABNAS_YAML_DIR=$yaml" >>"$GITHUB_ENV"
fi
echo "export TABNAS_FIXTURES_DIR=target/fixtures"
echo "export TABNAS_YAML_DIR=$yaml"

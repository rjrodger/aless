#!/usr/bin/env bash
# Build the documentation site into target/site, serve it on this machine,
# and open it in the browser: scripts/site.sh [PORT], PORT 8000 by default.
# Ctrl+C stops the server. Run it again after an edit to rebuild.
set -euo pipefail

cd "$(dirname "$0")/.."

port="${1:-8000}"
out="$PWD/target/site"
url="http://127.0.0.1:$port/"

# A page taken out of docs/ leaves the build too.
rm -rf "$out"
echo "site: building into target/site" >&2
ALESS_SITE_OUT="$out" cargo test --locked --quiet --test site the_site_builds >&2

if ! command -v python3 >/dev/null; then
  echo "site: python3 serves the site, and there is none here." >&2
  echo "site: open $out/index.html in a browser instead." >&2
  exit 1
fi

in_use="site: port $port is in use. Give another, as in scripts/site.sh 8001."
if curl -s -o /dev/null "$url" 2>/dev/null; then
  echo "$in_use" >&2
  exit 1
fi

python3 -m http.server "$port" --bind 127.0.0.1 --directory "$out" >&2 &
server=$!
trap 'kill "$server" 2>/dev/null || true' EXIT

# The page opens once this server answers, so the browser never meets a
# refused connection, or another server on the same port.
for _ in $(seq 50); do
  curl -fsS -o /dev/null "$url" 2>/dev/null && break
  sleep 0.1
done
if ! kill -0 "$server" 2>/dev/null; then
  echo "$in_use" >&2
  exit 1
fi

echo "site: serving $url, Ctrl+C stops it" >&2
# By the system, not by what is installed: on Debian `open` is openvt.
if [ "$(uname)" = Darwin ]; then
  open "$url"
elif command -v xdg-open >/dev/null; then
  xdg-open "$url" >/dev/null 2>&1 || true
fi
wait "$server"

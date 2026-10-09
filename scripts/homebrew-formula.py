#!/usr/bin/env python3
"""homebrew-formula.py: aless's Homebrew formula, from the one dist writes.

    python3 scripts/homebrew-formula.py aless.rb > Formula/aless.rb

dist's formula installs the binary and puts every other file of the
archive in $(brew --prefix)/share/aless, the man page and the shells'
completions among them, where neither man nor a shell looks for them.
This adds what a formula in homebrew-core has:

- the man page and the completions installed where Homebrew's formulas
  put theirs (man1, bash_completion, zsh_completion, fish_completion and
  pwsh_completion);
- a test that runs aless, for `brew test aless`.

publish-homebrew.yml runs this at each release, on the formula dist wrote
for it, and pushes the result to rjrodger/homebrew-tap. When the formula
is not shaped as dist 0.33.0 writes it, this fails rather than write one
without them: a new dist's formula needs this read again.
"""
import sys

# dist's install method ends by linking the binary's aliases, then moving
# whatever is left of the archive to pkgshare. The extras go between the
# two, so that what they install is no longer left over.
ANCHOR = "    install_binary_aliases!\n"

EXTRAS = """
    # The man page and the completions, where Homebrew's formulas put theirs.
    man1.install "man/aless.1"
    bash_completion.install "completions/aless.bash" => "aless"
    zsh_completion.install "completions/_aless"
    fish_completion.install "completions/aless.fish"
    pwsh_completion.install "completions/_aless.ps1"
"""

TEST = """
  test do
    assert_match "aless #{version}", shell_output("#{bin}/aless --version")
    assert_equal '{"a":[1,2]}', pipe_output("#{bin}/aless --json --compact", '{"a": [1, 2]}').strip
  end
"""


def formula(text: str) -> str:
    if text.count(ANCHOR) != 1:
        raise SystemExit(
            "homebrew-formula: dist's formula does not call install_binary_aliases! "
            "once, as dist 0.33.0's does; read this script against the new formula"
        )
    if "man1.install" in text or "  test do\n" in text:
        raise SystemExit("homebrew-formula: the formula already installs a man page or has a test")
    if not text.endswith("\n  end\nend\n"):
        raise SystemExit("homebrew-formula: the formula does not end as dist 0.33.0's does")
    text = text.replace(ANCHOR, ANCHOR + EXTRAS, 1)
    # The test goes last in the class, after the install method.
    return text[: -len("end\n")] + TEST + "end\n"


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: homebrew-formula.py FORMULA.rb")
    with open(sys.argv[1], encoding="utf-8") as f:
        text = f.read()
    sys.stdout.write(formula(text))


if __name__ == "__main__":
    main()

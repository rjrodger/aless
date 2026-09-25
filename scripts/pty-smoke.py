#!/usr/bin/env python3
"""Drive the aless binary in a pseudo-terminal and check what it draws.

Usage: scripts/pty-smoke.py [path/to/aless]

Opens two fixture copies in a scratch directory, walks the tree, opens
the help, switches tabs, edits a file behind the viewer's back and waits
for the reload, then quits. Exit status 0 means every expectation held.
Unix only (it needs a pty); Windows relies on the headless tests.
"""
import fcntl
import os
import pty
import select
import shutil
import struct
import sys
import tempfile
import termios
import time

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BIN = sys.argv[1] if len(sys.argv) > 1 else os.path.join(ROOT, "target", "debug", "aless")


def main():
    work = tempfile.mkdtemp(prefix="aless-smoke-")
    nested = os.path.join(work, "nested.json")
    yaml = os.path.join(work, "sample.yaml")
    shutil.copy(os.path.join(ROOT, "tests/fixtures/nested.json"), nested)
    shutil.copy(os.path.join(ROOT, "tests/fixtures/sample.yaml"), yaml)

    pid, fd = pty.fork()
    if pid == 0:
        os.environ["TERM"] = "xterm-256color"
        os.chdir(work)
        os.execv(BIN, [BIN, "--no-mouse", nested, yaml])
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 100, 0, 0))

    seen = bytearray()
    failures = []

    def read(timeout):
        end = time.time() + timeout
        while time.time() < end:
            r, _, _ = select.select([fd], [], [], 0.05)
            if r:
                try:
                    chunk = os.read(fd, 65536)
                except OSError:
                    return
                if not chunk:
                    return
                seen.extend(chunk)

    def send(text):
        os.write(fd, text.encode())
        read(0.4)

    def expect(label, *needles, timeout=3.0):
        end = time.time() + timeout
        while time.time() < end:
            text = seen.decode("utf-8", "replace")
            if all(n in text for n in needles):
                print(f"ok   {label}")
                return True
            read(0.1)
        print(f"FAIL {label}: expected {needles!r}")
        failures.append(label)
        return False

    read(1.5)
    expect("draws both tabs", "1:nested.json", "2:sample.yaml")
    expect("draws the yaml tree (active tab is the first)", "store")
    expect("status bar shows the format and watching", "json", "watching")
    send("j")
    send("l")
    expect("moving into store shows its path", ".store")
    send("j")
    send("h")
    send("h")
    expect("collapsing store shows a preview", "(4) {")
    send("l")
    send("/TAPL\r")
    expect("search lands on the TAPL title", "books[1].title", "[1/1]")
    send("yp")
    expect("yank reports where the path went", "Copied path to")
    send("\t")
    expect("Tab switches to the yaml tab", ".", "yaml")
    send("\t")

    # Edit the file behind the viewer's back.
    with open(nested) as f:
        text = f.read()
    time.sleep(0.3)
    with open(nested, "w") as f:
        f.write(text.replace('"TAPL"', '"TAPL, 2nd edition"'))
    expect("the change is picked up and reloaded", "Reloaded nested.json", "2nd edition", timeout=6.0)
    expect("the focus stayed on the edited title", "books[1].title")

    send("s")
    expect("source view shows the raw text", "(source)")
    send("s")
    send(":help\r")
    expect("help overlay opens", "aless help", "FOLDING")
    send("q")
    send("q")
    send("q")
    read(0.5)
    try:
        _, status = os.waitpid(pid, 0)
    except ChildProcessError:
        status = 0
    code = os.waitstatus_to_exitcode(status)
    if code != 0:
        print(f"FAIL exit status {code}")
        failures.append("exit")
    else:
        print("ok   clean exit")
    shutil.rmtree(work, ignore_errors=True)
    if failures:
        print("\n--- last screen ---")
        print(seen.decode("utf-8", "replace")[-3000:])
        sys.exit(1)
    print("all green")


if __name__ == "__main__":
    main()

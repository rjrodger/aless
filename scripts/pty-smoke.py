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
BIN = os.path.abspath(sys.argv[1]) if len(sys.argv) > 1 else os.path.join(ROOT, "target", "debug", "aless")


def drive(args, cwd, steps, size=(24, 100)):
    """Run aless with `args`, feed it `steps` and return (failures, transcript).

    Each step is (label, keys_to_send, expected_substrings, timeout).
    """
    pid, fd = pty.fork()
    if pid == 0:
        os.environ["TERM"] = "xterm-256color"
        os.chdir(cwd)
        os.execv(BIN, [BIN, "--no-mouse"] + args)
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", size[0], size[1], 0, 0))
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

    read(1.5)
    for label, keys, needles, timeout in steps:
        if callable(keys):
            # A change to make on disk behind the viewer's back.
            keys()
            read(0.4)
        elif keys:
            try:
                os.write(fd, keys.encode())
            except OSError as e:
                print(f"FAIL {label}: sending {keys!r}: {e} (did aless exit?)")
                failures.append(label)
                break
            read(0.4)
        end = time.time() + timeout
        ok = False
        while time.time() < end:
            text = seen.decode("utf-8", "replace")
            if all(n in text for n in needles):
                ok = True
                break
            read(0.1)
        print(("ok   " if ok else "FAIL ") + label + ("" if ok else f": expected {needles!r}"))
        if not ok:
            failures.append(label)
    read(0.5)
    try:
        _, status = os.waitpid(pid, 0)
        code = os.waitstatus_to_exitcode(status)
    except ChildProcessError:
        code = 0
    if code != 0:
        print(f"FAIL exit status {code}")
        failures.append("exit")
    else:
        print("ok   clean exit")
    return failures, seen.decode("utf-8", "replace")


def explorer_scenario(work):
    """Open the scratch directory in the explorer, enter a file, go up."""
    parent_name = os.path.basename(os.path.dirname(work))
    return drive([work], work, [
        ("explorer lists the directory", "", [os.path.basename(work) + "/", "nested.json", "sample.yaml", "directory"], 3.0),
        ("Enter on a file opens it in a second tab", "j\r", ["2:nested.json", "store"], 3.0),
        ("q closes the file tab and returns to the explorer", "q", ["sample.yaml"], 3.0),
        ("- climbs to the parent directory", "-", [parent_name + "/"], 3.0),
        ("quit", "q", [], 1.0),
    ])


def errors_scenario(work):
    """Open a file that does not parse, read the engine's report, fix it."""
    bad = os.path.join(work, "broken.json")
    with open(bad, "w") as f:
        f.write('{"a": 1,\n "b": [1, 2,,]\n}\n')

    def fix():
        time.sleep(0.3)
        with open(bad, "w") as f:
            f.write('{"a": 1,\n "b": [1, 2, 3]\n}\n')

    def rebreak():
        time.sleep(0.3)
        with open(bad, "w") as f:
            f.write('{"a": 1,\n "b": [1, 2 3]\n}\n')

    return drive([bad], work, [
        ("a file that does not parse shows the engine's report", "",
         ["[tabnas/unexpected]", "broken.json:2:13", "^ unexpected character(s): ,"], 3.0),
        ("! opens the full report", "!", ["error report", "do not match any rule alternative"], 3.0),
        ("any key returns", "x", ["broken.json:2:13"], 3.0),
        ("fixing the file shows the document", fix, ["Reloaded broken.json"], 6.0),
        ("breaking it again docks the report under the document", rebreak,
         ["parse error · ! shows the full report", "unexpected character(s): 3"], 6.0),
        ("quit", "q", [], 1.0),
    ])


def no_terminal_scenario(work):
    """A pty for standard output but no terminal to read keys from, as some
    agent harnesses run commands: the viewer must refuse at once, draw
    nothing, and name the options that work; with TERM=dumb, aless prints
    the document as JSON instead."""
    import json
    import subprocess
    failures = []
    target = os.path.join(work, "nested.json")
    for term, want_code in (("xterm-256color", 2), ("dumb", 0)):
        master, slave = pty.openpty()
        env = dict(os.environ, TERM=term)
        # A new session has no controlling terminal: /dev/tty cannot open.
        child = subprocess.Popen([BIN, target], stdin=subprocess.DEVNULL, stdout=slave,
                                 stderr=subprocess.PIPE, start_new_session=True, env=env)
        try:
            code = child.wait(timeout=10)
        except subprocess.TimeoutExpired:
            child.kill()
            code = "hung"
        # Keep our end of the slave open until the output is read: on macOS
        # the last close of a pty's slave discards what is still unread.
        drawn = bytearray()
        while select.select([master], [], [], 0.2)[0]:
            try:
                chunk = os.read(master, 65536)
            except OSError:
                break
            if not chunk:
                break
            drawn.extend(chunk)
        os.close(slave)
        os.close(master)
        err = child.stderr.read().decode("utf-8", "replace")
        label = f"no controlling terminal, TERM={term}"
        if code != want_code:
            print(f"FAIL {label}: exit {code}, wanted {want_code}; stderr: {err}")
            failures.append(label)
        elif want_code == 2 and (drawn or "--json" not in err):
            print(f"FAIL {label}: drew {bytes(drawn[:80])!r}, said {err!r}")
            failures.append(label)
        elif want_code == 0:
            try:
                doc = json.loads(drawn.decode().replace("\r\n", "\n"))
                assert doc["version"] == 3
            except Exception as e:
                print(f"FAIL {label}: not the document as JSON ({e}): {bytes(drawn[:80])!r}")
                failures.append(label)
            else:
                print(f"ok   {label}: prints JSON")
        else:
            print(f"ok   {label}: refuses at once, draws nothing, points at --json")
    return failures, ""


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
        try:
            os.write(fd, text.encode())
        except OSError as e:
            # EIO: the child has gone away.
            print(f"FAIL sending {text!r}: {e} (did aless exit?)")
            failures.append("send")
            return
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

    if not os.path.exists(BIN):
        print(f"FAIL no binary at {BIN}")
        sys.exit(1)
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
    for scenario in (explorer_scenario, errors_scenario, no_terminal_scenario):
        if failures:
            break
        more, transcript = scenario(work)
        if more:
            failures.extend(more)
            seen.extend(transcript.encode())
    shutil.rmtree(work, ignore_errors=True)
    if failures:
        print("\n--- last screen ---")
        print(seen.decode("utf-8", "replace")[-3000:])
        sys.exit(1)
    print("all green")


if __name__ == "__main__":
    main()

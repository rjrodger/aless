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
import codecs
import unicodedata

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BIN = os.path.abspath(sys.argv[1]) if len(sys.argv) > 1 else os.path.join(ROOT, "target", "debug", "aless")


class Screen:
    """Just enough of a terminal to read what aless draws.

    aless writes only the cells that changed since the last frame, with a
    cursor move before each run of them, so a word on screen need not be
    contiguous in the byte stream. This applies the stream the way a
    terminal does: cursor moves, erases, line feeds, and printable text,
    a wide character taking two cells and a combining one joining the cell
    before it. Every other escape sequence (colours, modes, OSC 52) is
    skipped. `seen` holds the text of every screen drawn so far, so that a
    check asks, as the stream search before it did, whether the text has
    been on screen.
    """

    def __init__(self, rows, cols):
        self.rows, self.cols = rows, cols
        self.grid = [[" "] * cols for _ in range(rows)]
        self.y = self.x = 0
        self.decoder = codecs.getincrementaldecoder("utf-8")("replace")
        self.pending = ""
        self.seen = []

    def feed(self, data):
        text = self.pending + self.decoder.decode(bytes(data))
        self.pending = ""
        i = 0
        while i < len(text):
            c = text[i]
            if c == "\x1b":
                end = self.escape(text, i)
                if end is None:
                    # The rest of the sequence is in the next chunk.
                    self.pending = text[i:]
                    break
                i = end
                continue
            if c == "\r":
                self.x = 0
            elif c == "\n":
                self.line_feed()
            elif c == "\b":
                self.x = max(0, self.x - 1)
            elif c >= " ":
                self.put(c)
            i += 1
        now = self.text()
        if not self.seen or self.seen[-1] != now:
            self.seen.append(now)

    def escape(self, text, i):
        """Apply the sequence at `text[i]`; the index past it, or None."""
        if i + 1 >= len(text):
            return None
        kind = text[i + 1]
        if kind == "[":
            j = i + 2
            while j < len(text) and not ("@" <= text[j] <= "~"):
                j += 1
            if j >= len(text):
                return None
            self.csi(text[i + 2:j], text[j])
            return j + 1
        if kind == "]":
            # OSC, to BEL or ST: a title or the clipboard, never drawn.
            j = i + 2
            while j < len(text):
                if text[j] == "\x07":
                    return j + 1
                if text[j] == "\x1b" and j + 1 < len(text) and text[j + 1] == "\\":
                    return j + 2
                j += 1
            return None
        return i + 2

    def csi(self, params, final):
        private = params.startswith("?")
        nums = [int(p) if p.isdigit() else 0 for p in params.lstrip("?").split(";")]
        n = nums[0] or 1
        if private:
            if final == "h" and 1049 in nums:
                self.clear(0, 0, self.rows, self.cols)
            return
        if final in "Hf":
            row = nums[0] or 1
            col = nums[1] if len(nums) > 1 and nums[1] else 1
            self.y, self.x = min(row, self.rows) - 1, min(col, self.cols) - 1
        elif final == "A":
            self.y = max(0, self.y - n)
        elif final == "B":
            self.y = min(self.rows - 1, self.y + n)
        elif final == "C":
            self.x = min(self.cols - 1, self.x + n)
        elif final == "D":
            self.x = max(0, self.x - n)
        elif final == "G":
            self.x = min(n, self.cols) - 1
        elif final == "d":
            self.y = min(n, self.rows) - 1
        elif final == "J":
            if nums[0] == 0:
                self.clear(self.y, self.x, self.y + 1, self.cols)
                self.clear(self.y + 1, 0, self.rows, self.cols)
            elif nums[0] == 1:
                self.clear(0, 0, self.y, self.cols)
                self.clear(self.y, 0, self.y + 1, self.x + 1)
            else:
                self.clear(0, 0, self.rows, self.cols)
        elif final == "K":
            if nums[0] == 0:
                self.clear(self.y, self.x, self.y + 1, self.cols)
            elif nums[0] == 1:
                self.clear(self.y, 0, self.y + 1, self.x + 1)
            else:
                self.clear(self.y, 0, self.y + 1, self.cols)
        elif final == "X":
            self.clear(self.y, self.x, self.y + 1, min(self.cols, self.x + n))

    def clear(self, top, left, bottom, right):
        for y in range(max(0, top), min(self.rows, bottom)):
            for x in range(max(0, left), min(self.cols, right)):
                self.grid[y][x] = " "

    def line_feed(self):
        if self.y + 1 < self.rows:
            self.y += 1
        else:
            self.grid.pop(0)
            self.grid.append([" "] * self.cols)

    def put(self, c):
        if unicodedata.combining(c) or c in "\u200d\ufe0f":
            # Joins the character before it.
            px = self.x - 1
            while px > 0 and self.grid[self.y][px] == "":
                px -= 1
            if px >= 0:
                self.grid[self.y][px] += c
            return
        width = 2 if unicodedata.east_asian_width(c) in "WF" else 1
        if self.x + width > self.cols:
            # Past the right margin, as a terminal wraps.
            self.x = 0
            self.line_feed()
        self.grid[self.y][self.x] = c
        if width == 2:
            self.grid[self.y][self.x + 1] = ""
        self.x += width

    def text(self):
        return "\n".join("".join(row).rstrip() for row in self.grid)

    def history(self):
        return "\n".join(self.seen)


def drive(args, cwd, steps, size=(24, 100), stdin_text=None):
    """Run aless with `args`, feed it `steps` and return (failures, transcript).

    Each step is (label, keys_to_send, expected_substrings, timeout). With
    `stdin_text`, standard input is a pipe holding it, as in `cmd | aless`,
    and the keys still come through the terminal.
    """
    pid, fd = pty.fork()
    if pid == 0:
        os.environ["TERM"] = "xterm-256color"
        os.chdir(cwd)
        if stdin_text is not None:
            r, w = os.pipe()
            os.write(w, stdin_text.encode())
            os.close(w)
            os.dup2(r, 0)
            os.close(r)
        os.execv(BIN, [BIN, "--no-mouse"] + args)
    fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", size[0], size[1], 0, 0))
    screen = Screen(size[0], size[1])
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
                screen.feed(chunk)

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
            text = screen.history()
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
    return failures, screen.text()


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


def too_large_scenario(work):
    """A file over --max-size opens as a tab that says so, and why."""
    big = os.path.join(work, "big.json")
    with open(big, "w") as f:
        f.write("[" + ", ".join(str(i) for i in range(1000)) + "]")
    return drive(["--max-size", "1K", big], work, [
        ("a file over --max-size shows why it was not read",
         "", ["[aless/too_large]", "over the 1.0 KB limit", "--max-size"], 3.0),
        ("quit", "q", [], 1.0),
    ])


def panes_scenario(work):
    """--panes out: the document beside its output, C-w and s in the
    output pane, q closing it."""
    data = os.path.join(work, "panes.csv")
    with open(data, "w") as f:
        f.write("name,age\nada,36\n")
    return drive(["--panes", "out", "--render", "yaml", data], work, [
        ("the input and its output side by side", "",
         [" input · panes.csv · tree", " output · panes.csv → yaml · tree"], 3.0),
        ("C-w and s show the output's text", "\x17s",
         ['- "name": "ada"', "panes.csv → yaml  (source)"], 3.0),
        ("q closes the output pane", "q", ["panes.csv ."], 3.0),
        ("quit", "q", [], 1.0),
    ])


def piped_input_scenario(work):
    """`command | aless` in a terminal: the document comes from the pipe and
    the keys from the terminal, so the viewer starts."""
    return drive([], work, [
        ("piped input opens in the viewer", "", ["(stdin)", "piped"], 3.0),
        ("keys reach it through the terminal", "j", [".piped"], 3.0),
        ("quit", "q", [], 1.0),
    ], stdin_text='{"piped": [1, 2, 3]}')


def no_terminal_scenario(work):
    """A pty for standard output but no terminal to read keys from, as some
    agent harnesses run commands: the viewer must refuse at once, draw
    nothing, and name the options that work; with TERM=dumb, aless prints
    the document as JSON instead."""
    import json
    import subprocess
    failures = []
    target = os.path.join(work, "nested.json")
    fifo = os.path.join(work, "never-written.fifo")
    os.mkfifo(fifo)
    grammar_fifo = os.path.join(work, "never-written.abnf")
    os.mkfifo(grammar_fifo)
    cases = (
        ("TERM=xterm-256color", "xterm-256color", [target], subprocess.DEVNULL, 2),
        # Input that never ends must not be read first: the refusal comes
        # before any input is opened.
        ("TERM=xterm-256color, stdin a pipe that stays open", "xterm-256color", [],
         subprocess.PIPE, 2),
        ("TERM=xterm-256color, a FIFO nobody writes", "xterm-256color", [fifo],
         subprocess.DEVNULL, 2),
        # A grammar file is input too: it is not opened before the refusal.
        ("TERM=xterm-256color, a --grammar FIFO nobody writes", "xterm-256color",
         ["--grammar", f"kv={grammar_fifo}", target], subprocess.DEVNULL, 2),
        ("TERM=dumb", "dumb", [target], subprocess.DEVNULL, 0),
    )
    for name, term, args, stdin, want_code in cases:
        master, slave = pty.openpty()
        env = dict(os.environ, TERM=term)
        # A new session has no controlling terminal: /dev/tty cannot open.
        child = subprocess.Popen([BIN] + args, stdin=stdin, stdout=slave,
                                 stderr=subprocess.PIPE, start_new_session=True, env=env)
        try:
            code = child.wait(timeout=10)
        except subprocess.TimeoutExpired:
            child.kill()
            child.wait()
            code = "hung"
        if child.stdin:
            child.stdin.close()
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
        label = f"no controlling terminal, {name}"
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

    screen = Screen(24, 100)
    failures = []
    last = [""]

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
                screen.feed(chunk)

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
            text = screen.history()
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
    last[0] = screen.text()
    for scenario in (explorer_scenario, errors_scenario, too_large_scenario,
                     panes_scenario, piped_input_scenario, no_terminal_scenario):
        if failures:
            break
        more, last_screen = scenario(work)
        if more:
            failures.extend(more)
            last[0] = last_screen
    shutil.rmtree(work, ignore_errors=True)
    if failures:
        print("\n--- last screen ---")
        print(last[0])
        sys.exit(1)
    print("all green")


if __name__ == "__main__":
    main()

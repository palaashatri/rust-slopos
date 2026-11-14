#!/usr/bin/env python3
"""Drive a running X11 session's Calculator through keyboard, pointer and AT-SPI.

Run by scripts/run-calculator-qa.sh inside a private Xvfb + Openbox + AT-SPI
session; it is not meant to be started on its own. The script launches
Calculator the way a menu would, from its desktop entry, then checks:

- the accessibility tree (window, result label, 19 named buttons);
- typed sums, Escape, Backspace and F9 (change sign);
- Tab focus traversal, reported through AT-SPI, and Space on the focused key;
- pointer clicks on the centres AT-SPI reports for each key;
- the AT-SPI Click action;
- a live window capture against the app's own --snapshot at the same scale;
- that closing the window through the window manager ends the process.

Every check prints PASS or FAIL; the exit status is the number of failures.
"""

import argparse
import configparser
import os
import shlex
import shutil
import subprocess
import sys
import time

import gi

gi.require_version("Atspi", "2.0")
from gi.repository import Atspi  # noqa: E402

BUTTON_NAMES = [
    "Clear", "Change sign", "Percent", "Divide",
    "7", "8", "9", "Multiply",
    "4", "5", "6", "Subtract",
    "1", "2", "3", "Add",
    "0", "Decimal point", "Equals",
]

failures = []


def check(name, ok, detail=""):
    print(f"{'PASS' if ok else 'FAIL'} {name}{': ' + detail if detail else ''}", flush=True)
    if not ok:
        failures.append(name)
    return ok


def run(*argv, check_exit=True):
    result = subprocess.run(argv, capture_output=True, text=True)
    if check_exit and result.returncode != 0:
        raise RuntimeError(f"{' '.join(argv)} failed: {result.stderr.strip()}")
    return result.stdout.strip()


def wait_for(predicate, timeout=10.0, step=0.1):
    deadline = time.monotonic() + timeout
    while True:
        value = predicate()
        if value or time.monotonic() > deadline:
            return value
        time.sleep(step)


def desktop_command(desktop_file, bin_dir):
    """The argv a launcher would run for this entry, with TryExec checked."""
    parser = configparser.ConfigParser(interpolation=None, strict=False)
    parser.optionxform = str
    parser.read(desktop_file, encoding="utf-8")
    entry = parser["Desktop Entry"]
    check("desktop entry is an application", entry.get("Type") == "Application")
    check("desktop entry runs without a terminal", entry.get("Terminal", "false") == "false")
    argv = [part for part in shlex.split(entry["Exec"]) if not part.startswith("%")]
    path = os.pathsep.join([bin_dir, os.environ.get("PATH", "")])
    try_exec = entry.get("TryExec")
    if try_exec:
        check("TryExec resolves in the package", shutil.which(try_exec, path=bin_dir) is not None, try_exec)
    resolved = shutil.which(argv[0], path=path)
    check("Exec resolves to the packaged binary",
          resolved is not None and os.path.dirname(os.path.realpath(resolved)) == os.path.realpath(bin_dir),
          str(resolved))
    return [resolved or argv[0], *argv[1:]]


def find_app():
    desktop = Atspi.get_desktop(0)
    for index in range(desktop.get_child_count()):
        app = desktop.get_child_at_index(index)
        if app is None:
            continue
        for window_index in range(app.get_child_count()):
            window = app.get_child_at_index(window_index)
            if window is not None and window.get_name() == "Calculator":
                return window
    return None


def nodes(window):
    found = {}
    result = None
    for index in range(window.get_child_count()):
        child = window.get_child_at_index(index)
        name = child.get_name()
        role = child.get_role_name()
        if role in ("push button", "button"):
            found[name] = child
        elif role == "label":
            result = child
    return result, found


def result_text(window):
    label, _ = nodes(window)
    return label.get_name() if label else None


def expect_result(window, expected, name):
    want = f"Result {expected}"
    got = wait_for(lambda: result_text(window) == want and want, timeout=5)
    check(name, bool(got), f"expected {want!r}, got {result_text(window)!r}")


def focused_button(window):
    _, buttons = nodes(window)
    for name, node in buttons.items():
        if node.get_state_set().contains(Atspi.StateType.FOCUSED):
            return name
    return None


def key(xid, *keys):
    run("xdotool", "key", "--window", xid, *keys)


def type_text(xid, text):
    run("xdotool", "type", "--window", xid, text)


def click_button(window, xid, name):
    _, buttons = nodes(window)
    extents = buttons[name].get_extents(Atspi.CoordType.WINDOW)
    x = extents.x + extents.width // 2
    y = extents.y + extents.height // 2
    run("xdotool", "mousemove", "--window", xid, str(x), str(y), "click", "1")
    return extents


def capture(xid, path):
    run("import", "-window", xid, path)


def pixel_difference(first, second):
    result = subprocess.run(["compare", "-metric", "AE", first, second, "null:"],
                            capture_output=True, text=True)
    try:
        return int(float(result.stderr.split()[0]))
    except (ValueError, IndexError):
        return -1


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--bin-dir", required=True)
    parser.add_argument("--desktop-file", required=True)
    parser.add_argument("--scale", type=int, default=1)
    parser.add_argument("--out", required=True)
    args = parser.parse_args()
    os.makedirs(args.out, exist_ok=True)
    tag = f"{args.scale}x"
    os.environ["SLOPOS_SCALE"] = str(args.scale)

    command = desktop_command(args.desktop_file, args.bin_dir)
    log = open(os.path.join(args.out, f"calculator-{tag}.log"), "w")
    process = subprocess.Popen(command, stdout=log, stderr=subprocess.STDOUT)
    try:
        xid = wait_for(lambda: run("xdotool", "search", "--name", "^Calculator$",
                                   check_exit=False).split("\n")[0], timeout=15)
        if not check("window appears", bool(xid)):
            return
        run("xdotool", "windowactivate", "--sync", xid, check_exit=False)
        window = wait_for(find_app, timeout=15)
        if not check("AT-SPI exposes the Calculator window", window is not None):
            return

        label, buttons = nodes(window)
        check("result label is exposed", label is not None and label.get_name() == "Result 0",
              label.get_name() if label else "missing")
        check("all 19 keys are exposed as named buttons",
              sorted(buttons) == sorted(BUTTON_NAMES), ", ".join(sorted(buttons)))

        # Visual: the live window matches the app's own render of the same state.
        live = os.path.join(args.out, f"calculator-live-{tag}.png")
        snapshot = os.path.join(args.out, f"calculator-snapshot-{tag}.png")
        time.sleep(0.3)
        capture(xid, live)
        env = dict(os.environ, SLOPOS_SCALE=str(args.scale))
        subprocess.run([command[0], "--snapshot", snapshot], env=env, check=True)
        geometry = run("identify", "-format", "%wx%h", live)
        expected = run("identify", "-format", "%wx%h", snapshot)
        check("window size matches the snapshot", geometry == expected, f"{geometry} vs {expected}")
        diff = pixel_difference(live, snapshot)
        check("live window is pixel-identical to --snapshot", diff == 0, f"{diff} pixels differ")

        # Keyboard.
        type_text(xid, "7*6=")
        expect_result(window, "42", "typed 7*6= shows 42")
        capture(xid, os.path.join(args.out, f"calculator-42-{tag}.png"))
        key(xid, "Escape")
        expect_result(window, "0", "Escape clears")
        type_text(xid, "123")
        key(xid, "BackSpace")
        expect_result(window, "12", "Backspace removes the last digit")
        key(xid, "F9")
        expect_result(window, "-12", "F9 changes sign")
        type_text(xid, "+2")
        key(xid, "Return")
        expect_result(window, "-10", "Return evaluates")
        key(xid, "Delete")
        expect_result(window, "0", "Delete clears")
        type_text(xid, "1/0=")
        got = wait_for(lambda: result_text(window) not in (None, "Result 1", "Result 0") and result_text(window), timeout=5)
        check("division by zero reports an error instead of a number", bool(got) and "Result Error" in str(got), str(got))
        key(xid, "Escape")
        expect_result(window, "0", "Escape clears the error")

        # Focus traversal and activation.
        key(xid, "Tab")
        check("Tab focuses the first key", wait_for(lambda: focused_button(window) == "Clear", timeout=3), str(focused_button(window)))
        key(xid, "Tab", "Tab", "Tab", "Tab")
        check("four more Tabs reach 7", wait_for(lambda: focused_button(window) == "7", timeout=3), str(focused_button(window)))
        capture(xid, os.path.join(args.out, f"calculator-focus-{tag}.png"))
        key(xid, "space")
        expect_result(window, "7", "Space activates the focused key")
        key(xid, "shift+Tab")
        check("Shift+Tab moves focus back", wait_for(lambda: focused_button(window) == "Divide", timeout=3), str(focused_button(window)))
        key(xid, "Escape")
        expect_result(window, "0", "Escape clears after focus moves")

        # Pointer, at the centres AT-SPI reports.
        extents = click_button(window, xid, "9")
        check("AT-SPI key extents are scaled to the window",
              extents.width == 41 * args.scale and extents.height == 28 * args.scale,
              f"{extents.width}x{extents.height}")
        click_button(window, xid, "Add")
        click_button(window, xid, "1")
        click_button(window, xid, "Equals")
        expect_result(window, "10", "pointer clicks 9 + 1 = show 10")
        click_button(window, xid, "Clear")
        expect_result(window, "0", "pointer click on C clears")

        # AT-SPI action, as a screen reader or switch device would use it.
        _, buttons = nodes(window)
        action = buttons["5"].get_action_iface()
        check("keys expose a click action", action is not None and action.get_n_actions() > 0)
        if action is not None and action.get_n_actions() > 0:
            action.do_action(0)
            expect_result(window, "5", "AT-SPI click on 5 enters 5")

        # Super+Q (Openbox's Close in the SLOPOS rc.xml) ends the process cleanly.
        run("xdotool", "windowactivate", "--sync", xid, check_exit=False)
        run("xdotool", "key", "super+q")
        try:
            code = process.wait(timeout=10)
            check("Super+Q closes Calculator with exit status 0", code == 0, f"exit {code}")
        except subprocess.TimeoutExpired:
            check("Super+Q closes Calculator with exit status 0", False, "still running")
    finally:
        if process.poll() is None:
            process.kill()
            process.wait()
        log.close()
    with open(os.path.join(args.out, f"calculator-{tag}.log")) as handle:
        text = handle.read()
    check("no panic in the Calculator log", "panicked at" not in text)


if __name__ == "__main__":
    main()
    print(f"CALCULATOR_QA_FAILURES={len(failures)}")
    sys.exit(min(len(failures), 100))

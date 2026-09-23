#!/usr/bin/env python3
"""Verify SLOPOS GTK surfaces through the maintained AT-SPI 2 GI API."""

import subprocess
import sys
import time

import gi

gi.require_version("Atspi", "2.0")
from gi.repository import Atspi, GLib


EXPECTED_NAMES = {
    "SLOPOS top menu bar",
    "SLOPOS application search",
    "Application search field",
    "SLOPOS system settings",
    "SLOPOS software catalogue",
}


def collect(node, result, depth=0):
    if depth > 18:
        return
    try:
        name = node.get_name() or ""
        role = node.get_role_name() or ""
        states = node.get_state_set()
        result.append((name, role, states))
        child_count = max(0, node.get_child_count())
    except Exception as error:
        print(f"AT_SPI_NODE_ERROR={error!r}", file=sys.stderr)
        return

    for index in range(child_count):
        try:
            child = node.get_child_at_index(index)
        except Exception as error:
            print(f"AT_SPI_CHILD_ERROR={error!r}", file=sys.stderr)
            continue
        if child is not None:
            collect(child, result, depth + 1)


def collect_nodes(node, result, depth=0):
    """Collect live accessibles for interaction checks."""
    if depth > 18:
        return
    try:
        result.append(node)
        child_count = max(0, node.get_child_count())
    except Exception:
        return
    for index in range(child_count):
        try:
            child = node.get_child_at_index(index)
        except Exception:
            continue
        if child is not None:
            collect_nodes(child, result, depth + 1)


def pump_and_snapshot(desktop):
    context = GLib.MainContext.default()
    while context.pending():
        context.iteration(False)
    nodes = []
    collect_nodes(desktop, nodes)
    return nodes


def focused_names(desktop):
    names = []
    for node in pump_and_snapshot(desktop):
        try:
            if node.get_state_set().contains(Atspi.StateType.FOCUSED):
                name = node.get_name() or ""
                if name:
                    names.append(name)
        except Exception:
            continue
    return names


def find_named(desktop, name):
    for node in pump_and_snapshot(desktop):
        try:
            if (node.get_name() or "") == name:
                return node
        except Exception:
            continue
    return None


def text_value(node):
    """Read an editable value through the current AT-SPI Text interface."""
    try:
        text_iface = node.get_text_iface()
        if text_iface is not None:
            count = max(0, text_iface.get_character_count())
            if count == 0:
                return ""
            chars = [
                text_iface.get_character_at_offset(index) for index in range(count)
            ]
            normalized = []
            for char in chars:
                if isinstance(char, int):
                    normalized.append(chr(char))
                elif isinstance(char, bytes):
                    normalized.append(char.decode("utf-8"))
                elif isinstance(char, str):
                    normalized.append(char)
                else:
                    return None
            return "".join(normalized)
    except Exception:
        pass
    return None


def wait_for_focus(desktop, expected=None, excluded=None, timeout=5):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        names = focused_names(desktop)
        if expected is not None and expected in names:
            return names
        if excluded and any(name not in excluded for name in names):
            return names
        time.sleep(0.1)
    return focused_names(desktop)


def run_extended_checks(desktop):
    """Exercise UTF-8 Entry input and a reversible keyboard focus traversal."""
    search_windows = subprocess.check_output(
        ["xdotool", "search", "--onlyvisible", "--name", "^SLOPOS Search$"],
        text=True,
    ).split()
    if not search_windows:
        raise RuntimeError("SLOPOS Search window is not visible")
    search_window = search_windows[-1]
    subprocess.run(["xdotool", "windowactivate", "--sync", search_window], check=True)

    field_name = "Application search field"
    if field_name not in wait_for_focus(desktop, expected=field_name):
        raise RuntimeError("Search field did not receive initial focus")
    field = find_named(desktop, field_name)
    if field is None:
        raise RuntimeError("Search field disappeared from AT-SPI tree")

    subprocess.run(
        ["xclip", "-selection", "clipboard"], input="café", text=True, check=True
    )
    subprocess.run(["xdotool", "key", "--clearmodifiers", "ctrl+v"], check=True)
    typed = None
    deadline = time.monotonic() + 4
    while time.monotonic() < deadline:
        field = find_named(desktop, field_name)
        typed = text_value(field) if field is not None else None
        if typed == "café":
            break
        time.sleep(0.1)
    if typed != "café":
        raise RuntimeError(f"UTF-8 Search entry mismatch: {typed!r}")

    subprocess.run(["xdotool", "key", "ctrl+a", "BackSpace"], check=True)
    time.sleep(0.3)

    subprocess.run(["xdotool", "key", "Tab"], check=True)
    after_tab = wait_for_focus(desktop, excluded={field_name})
    if not any(name != field_name for name in after_tab):
        raise RuntimeError(f"Tab did not move focus from Search field: {after_tab!r}")

    subprocess.run(["xdotool", "key", "shift+Tab"], check=True)
    after_reverse = wait_for_focus(desktop, expected=field_name)
    if field_name not in after_reverse:
        raise RuntimeError(
            f"Shift+Tab did not return focus to Search field: {after_reverse!r}"
        )

    print(f"AT_SPI_UTF8_TEXT={typed}")
    print(f"AT_SPI_FOCUS_AFTER_TAB={after_tab}")
    print(f"AT_SPI_FOCUS_AFTER_SHIFT_TAB={after_reverse}")
    print("AT_SPI_EXTENDED_STATUS_0")


def main():
    Atspi.set_main_context(GLib.MainContext.default())
    Atspi.init()
    desktop = Atspi.get_desktop(0)
    desktop.set_cache_mask(Atspi.Cache.ALL)
    deadline = time.monotonic() + 15
    last = []
    while time.monotonic() < deadline:
        context = GLib.MainContext.default()
        while context.pending():
            context.iteration(False)
        last = []
        collect(desktop, last)
        names = {name for name, _, _ in last if name}
        if EXPECTED_NAMES.issubset(names):
            break
        time.sleep(0.5)

    names = {name for name, _, _ in last if name}
    missing = sorted(EXPECTED_NAMES - names)
    if missing:
        print(f"AT_SPI_MISSING={','.join(missing)}", file=sys.stderr)
        print(f"AT_SPI_SEEN={','.join(sorted(names))}", file=sys.stderr)
        if last:
            root_name, root_role, _ = last[0]
            print(f"AT_SPI_ROOT={root_role!r}:{root_name!r}", file=sys.stderr)
        return 1

    focused = [
        (name, role)
        for name, role, states in last
        if name == "Application search field"
        and states.contains(Atspi.StateType.FOCUSED)
    ]
    if not focused:
        print("AT_SPI_SEARCH_FOCUS_MISSING", file=sys.stderr)
        return 1

    for name, role, states in last:
        if name in EXPECTED_NAMES:
            print(
                f"AT_SPI role={role!r} name={name!r} "
                f"focused={states.contains(Atspi.StateType.FOCUSED)}"
            )
    print(f"AT_SPI_EXPECTED_NAMES={len(EXPECTED_NAMES)}")
    if "--extended" in sys.argv:
        run_extended_checks(desktop)
    print("AT_SPI_STATUS_0")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

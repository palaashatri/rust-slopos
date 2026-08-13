#!/usr/bin/env python3
"""Verify SLOPOS GTK surfaces through the maintained AT-SPI 2 GI API."""

import sys
import time

import gi

gi.require_version("Atspi", "2.0")
from gi.repository import Atspi, GLib


EXPECTED_NAMES = {
    "SLOPOS top bar",
    "SLOPOS application strip",
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

    # A transient application can disappear between count and lookup. Keep
    # walking its siblings instead of abandoning the complete desktop tree.
    for index in range(child_count):
        try:
            child = node.get_child_at_index(index)
        except Exception as error:
            print(f"AT_SPI_CHILD_ERROR={error!r}", file=sys.stderr)
            continue
        if child is not None:
            collect(child, result, depth + 1)


def main():
    Atspi.set_main_context(GLib.MainContext.default())
    Atspi.init()
    desktop = Atspi.get_desktop(0)
    desktop.set_cache_mask(Atspi.Cache.ALL)
    deadline = time.monotonic() + 15
    last = []
    while time.monotonic() < deadline:
        # AT-SPI routes D-Bus watches through GLib. Pump the context between
        # polls so newly registered applications are visible to this probe.
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
    print("AT_SPI_STATUS_0")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

#!/usr/bin/env python3
"""Verify Settings delegation and honest unavailable states through AT-SPI."""

import argparse
import os
import time

import gi

gi.require_version("Atspi", "2.0")
from gi.repository import Atspi, GLib


PANELS = [
    "Displays settings",
    "Sound settings",
    "Network settings",
    "Bluetooth settings",
    "Power settings",
    "Appearance settings",
    "Desktop settings",
    "Keyboard & Mouse settings",
]

# Appearance is deliberately SLOPOS-owned: it switches Platinum/Graphite and
# must remain available even on a minimal system. Every other panel delegates
# mutation to a mature upstream utility.
BUILT_IN = "Appearance settings"
DELEGATES = [
    ("Displays settings", "arandr"),
    ("Sound settings", "pavucontrol"),
    ("Network settings", "nm-connection-editor"),
    ("Bluetooth settings", "blueman-manager"),
    ("Power settings", "xfce4-power-manager-settings"),
    ("Desktop settings", "pcmanfm"),
    ("Keyboard & Mouse settings", "lxinput"),
]


def children(node, result, depth=0):
    if depth > 20:
        return
    try:
        result.append(node)
        count = max(0, node.get_child_count())
    except Exception:
        return
    for index in range(count):
        try:
            child = node.get_child_at_index(index)
        except Exception:
            continue
        if child is not None:
            children(child, result, depth + 1)


def snapshot(desktop):
    context = GLib.MainContext.default()
    while context.pending():
        context.iteration(False)
    nodes = []
    children(desktop, nodes)
    return nodes


def find_named(desktop, name):
    for node in snapshot(desktop):
        try:
            if (node.get_name() or "") == name:
                return node
        except Exception:
            continue
    return None


def state_is_enabled(node):
    states = node.get_state_set()
    return states.contains(Atspi.StateType.ENABLED) and states.contains(
        Atspi.StateType.SENSITIVE
    )


def wait_for_panels(desktop):
    deadline = time.monotonic() + 15
    while time.monotonic() < deadline:
        if all(find_named(desktop, name) is not None for name in PANELS):
            return
        time.sleep(0.25)
    missing = [name for name in PANELS if find_named(desktop, name) is None]
    raise RuntimeError(f"Settings panels missing from AT-SPI: {missing}")


def verify_builtin_appearance(desktop):
    node = find_named(desktop, BUILT_IN)
    if node is None or not state_is_enabled(node):
        raise RuntimeError("built-in Appearance panel must always be enabled")
    action = node.get_action_iface()
    if action is None or action.get_n_actions() < 1:
        raise RuntimeError("built-in Appearance panel has no AT-SPI action")
    print("SETTINGS_BUILTIN_APPEARANCE_ENABLED=1")


def verify_disabled(desktop):
    verify_builtin_appearance(desktop)
    for name, _ in DELEGATES:
        node = find_named(desktop, name)
        if state_is_enabled(node):
            raise RuntimeError(f"unavailable delegated panel is still enabled: {name}")
    print("SETTINGS_UNAVAILABLE_CONTROLS_DISABLED=7")


def verify_delegation(desktop):
    verify_builtin_appearance(desktop)
    for name, utility in DELEGATES:
        node = find_named(desktop, name)
        if not state_is_enabled(node):
            raise RuntimeError(f"available delegated panel is disabled: {name}")

        action = node.get_action_iface()
        if action is None or action.get_n_actions() < 1:
            raise RuntimeError(f"{name} has no AT-SPI action")
        if not action.do_action(0):
            raise RuntimeError(f"{name} action was rejected")

    probe_log = os.environ.get("SLOPOS_SERVICE_PROBE_LOG")
    if not probe_log:
        raise RuntimeError("SLOPOS_SERVICE_PROBE_LOG is required")
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        try:
            content = open(probe_log, encoding="utf-8").read()
        except FileNotFoundError:
            content = ""
        lines = set(content.splitlines())
        missing = [utility for _, utility in DELEGATES if utility not in lines]
        if not missing:
            print("SETTINGS_DELEGATED_CONTROLS=7")
            print("SETTINGS_DELEGATED_DISPLAY=arandr")
            return
        time.sleep(0.1)
    raise RuntimeError(f"delegated utilities were not invoked: {missing}")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--mode", choices=("disabled", "delegation"), required=True)
    args = parser.parse_args()

    Atspi.set_main_context(GLib.MainContext.default())
    Atspi.init()
    desktop = Atspi.get_desktop(0)
    desktop.set_cache_mask(Atspi.Cache.ALL)
    wait_for_panels(desktop)
    if args.mode == "disabled":
        verify_disabled(desktop)
    else:
        verify_delegation(desktop)
    print(f"SETTINGS_SERVICE_MODE={args.mode}")
    print("SETTINGS_SERVICE_QA_STATUS_0")


if __name__ == "__main__":
    main()

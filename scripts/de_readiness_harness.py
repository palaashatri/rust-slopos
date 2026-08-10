#!/usr/bin/env python3
"""Evidence-oriented SLOPOS-I application launch harness.

This harness intentionally does *not* infer interaction success from source
code, unit tests, or a mapped screenshot.  It can prove only process launch,
continued process liveness, environment/socket selection, and artifact capture.
Pointer-driven move/resize, focus, clipboard, audio, portals, and clean shutdown
remain ``UNTESTED`` until an external interaction driver records them.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import signal
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

NATIVE_APPS = ("finder", "textedit", "terminal", "settings", "appstore")
EXTERNAL_APPS = (
    ("firefox", "firefox"),
    ("mpv", "mpv"),
    ("doom", "chocolate-doom"),
    ("libreoffice", "libreoffice"),
    ("gtk3-demo", "gtk3-demo"),
    ("qt-demo", "qtdiag"),
    ("electron", "electron"),
    ("java", "java"),
    ("flatpak", "flatpak"),
)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--launch",
        action="store_true",
        help="actually start available native applications for a liveness sample",
    )
    parser.add_argument("--sample-seconds", type=float, default=3.0)
    parser.add_argument("--output-root", default="artifacts/de-readiness")
    parser.add_argument("--app", action="append", choices=NATIVE_APPS)
    return parser.parse_args()


def resolve_native_binary(name: str) -> Path | None:
    candidates = (
        Path("target/release") / name,
        Path("target/debug") / name,
        Path.home() / ".local/bin" / name,
        Path("/usr/local/bin") / name,
        Path("/usr/bin") / name,
    )
    for candidate in candidates:
        if candidate.is_file() and os.access(candidate, os.X_OK):
            return candidate.resolve()
    path = shutil.which(name)
    return Path(path).resolve() if path else None


def process_snapshot(pid: int) -> dict[str, Any]:
    result: dict[str, Any] = {"pid": pid}
    try:
        completed = subprocess.run(
            ["ps", "-o", "pid=,ppid=,stat=,%cpu=,%mem=,rss=,etime=,args=", "-p", str(pid)],
            text=True,
            capture_output=True,
            check=False,
        )
        result["ps"] = completed.stdout.strip()
    except OSError as error:
        result["ps_error"] = str(error)
    return result


def capture_screenshot(destination: Path) -> dict[str, Any]:
    grim = shutil.which("grim")
    if not grim:
        return {"status": "UNAVAILABLE", "reason": "grim not installed"}
    completed = subprocess.run(
        [grim, str(destination)], text=True, capture_output=True, check=False
    )
    if completed.returncode == 0 and destination.is_file():
        return {"status": "CAPTURED", "path": str(destination)}
    return {
        "status": "FAILED",
        "returncode": completed.returncode,
        "stderr": completed.stderr.strip(),
    }


def terminate_process(process: subprocess.Popen[str]) -> dict[str, Any]:
    if process.poll() is not None:
        return {"requested": False, "returncode": process.returncode}
    process.send_signal(signal.SIGTERM)
    try:
        returncode = process.wait(timeout=2.0)
        return {"requested": True, "signal": "SIGTERM", "returncode": returncode}
    except subprocess.TimeoutExpired:
        process.kill()
        returncode = process.wait(timeout=2.0)
        return {"requested": True, "signal": "SIGKILL", "returncode": returncode}


def test_native_app(name: str, output_dir: Path, launch: bool, sample_seconds: float) -> dict[str, Any]:
    app_dir = output_dir / "applications" / name
    app_dir.mkdir(parents=True, exist_ok=True)
    binary = resolve_native_binary(name)
    result: dict[str, Any] = {
        "application": name,
        "binary": str(binary) if binary else None,
        "launch_status": "NOT_FOUND" if binary is None else "NOT_RUN",
        "process_liveness": "UNTESTED",
        "window_mapped": "UNTESTED",
        "pointer_move_resize": "UNTESTED",
        "keyboard_focus": "UNTESTED",
        "clipboard": "UNTESTED",
        "audio": "UNTESTED",
        "portals": "UNTESTED",
        "clean_exit": "UNTESTED",
    }
    if binary is None or not launch:
        (app_dir / "result.json").write_text(json.dumps(result, indent=2) + "\n")
        return result

    env = os.environ.copy()
    command = [str(binary)]
    (app_dir / "command.txt").write_text(" ".join(command) + "\n")
    stdout_file = (app_dir / "stdout.log").open("w")
    stderr_file = (app_dir / "stderr.log").open("w")
    try:
        process = subprocess.Popen(
            command,
            env=env,
            text=True,
            stdin=subprocess.DEVNULL,
            stdout=stdout_file,
            stderr=stderr_file,
            start_new_session=True,
        )
    except OSError as error:
        result["launch_status"] = "LAUNCH_FAILED"
        result["launch_error"] = str(error)
        stdout_file.close()
        stderr_file.close()
        (app_dir / "result.json").write_text(json.dumps(result, indent=2) + "\n")
        return result

    time.sleep(max(0.1, sample_seconds))
    returncode = process.poll()
    if returncode is None:
        result["launch_status"] = "PROCESS_RUNNING"
        result["process_liveness"] = "OBSERVED"
        result["process"] = process_snapshot(process.pid)
        result["screenshot"] = capture_screenshot(app_dir / "launch.png")
        result["harness_termination"] = terminate_process(process)
        # Harness-requested termination cannot establish that the app's own
        # close workflow or unsaved-data prompts exit cleanly.
        result["clean_exit"] = "UNTESTED"
    else:
        result["launch_status"] = "EXITED_EARLY"
        result["returncode"] = returncode
        result["process_liveness"] = "FAILED"

    stdout_file.close()
    stderr_file.close()
    (app_dir / "result.json").write_text(json.dumps(result, indent=2) + "\n")
    return result


def main() -> int:
    args = parse_args()
    now = datetime.now(timezone.utc)
    timestamp = now.strftime("%Y%m%d_%H%M%S")
    output_dir = Path(args.output_root) / timestamp
    output_dir.mkdir(parents=True, exist_ok=True)

    environment = {
        "timestamp_utc": now.isoformat(),
        "launch_enabled": args.launch,
        "sample_seconds": args.sample_seconds,
        "xdg_session_type": os.environ.get("XDG_SESSION_TYPE"),
        "xdg_current_desktop": os.environ.get("XDG_CURRENT_DESKTOP"),
        "wayland_display": os.environ.get("WAYLAND_DISPLAY"),
        "slopos_client_wayland_display": os.environ.get("SLOPOS_CLIENT_WAYLAND_DISPLAY"),
        "slopos_host_wayland_display": os.environ.get("SLOPOS_HOST_WAYLAND_DISPLAY"),
        "display": os.environ.get("DISPLAY"),
        "user": os.environ.get("USER"),
        "hostname": os.uname().nodename,
        "methodology": "launch/liveness evidence only; interactions remain UNTESTED",
    }
    (output_dir / "environment.json").write_text(json.dumps(environment, indent=2) + "\n")

    ps = subprocess.run(
        ["ps", "-eo", "pid,ppid,stat,%cpu,%mem,rss,etime,args", "--sort=-%cpu"],
        text=True,
        capture_output=True,
        check=False,
    )
    (output_dir / "process-tree.txt").write_text(ps.stdout)

    requested_apps = tuple(args.app) if args.app else NATIVE_APPS
    results = [
        test_native_app(name, output_dir, args.launch, args.sample_seconds)
        for name in requested_apps
    ]

    external = [
        {
            "application": name,
            "binary": binary,
            "installed": shutil.which(binary) is not None,
            "runtime_status": "UNTESTED",
        }
        for name, binary in EXTERNAL_APPS
    ]
    (output_dir / "external-app-inventory.json").write_text(
        json.dumps(external, indent=2) + "\n"
    )

    lines = [
        "# SLOPOS-I DE Readiness Evidence Report",
        "",
        f"**Timestamp (UTC):** {now.isoformat()}",
        f"**Launch mode:** {'enabled' if args.launch else 'inventory only'}",
        "",
        "> This report does not claim that windows mapped, moved, resized, focused,",
        "> played audio, used portals, or exited cleanly unless a dedicated external",
        "> interaction test supplied that evidence. Those fields remain `UNTESTED`.",
        "",
        "## Native application launch/liveness",
        "",
        "| Application | Binary | Launch status | Liveness | Interaction |",
        "| --- | --- | --- | --- | --- |",
    ]
    for result in results:
        lines.append(
            f"| {result['application']} | {result['binary'] or 'missing'} | "
            f"{result['launch_status']} | {result['process_liveness']} | UNTESTED |"
        )
    lines.extend(["", "## External package inventory", ""])
    lines.extend(
        f"- **{row['application']}**: {'installed' if row['installed'] else 'not installed'}; runtime UNTESTED"
        for row in external
    )
    (output_dir / "report.md").write_text("\n".join(lines) + "\n")

    print(output_dir / "report.md")
    return 0


if __name__ == "__main__":
    sys.exit(main())

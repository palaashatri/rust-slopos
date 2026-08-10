#!/bin/sh
# Copyright (c) 2026 Palaash Atri
# SPDX-License-Identifier: MIT

# Linux-only runtime driver for the compositor-owned viewport gate. The
# compositor must have been started with SLOPOS_SHOT_PATH pointing at the PNG
# supplied here. This helper sends the compositor's SIGUSR1 readback request,
# waits for a new state/frame pair, then invokes the strict JSON/PNG validator.

set -eu

case "$(uname -s)" in
    Linux) ;;
    *)
        printf '%s\n' 'capture-viewport-runtime: Linux is required' >&2
        exit 2
        ;;
esac

usage() {
    printf '%s\n' \
        'usage: scripts/capture-viewport-runtime.sh --pid PID --state STATE --framebuffer PNG [--artifact JSON] [--timeout SECONDS]' \
        'The compositor must be running and configured with SLOPOS_SHOT_PATH=PNG.' >&2
}

pid=
state=
framebuffer=
artifact=
timeout_seconds=15

while [ "$#" -gt 0 ]; do
    case "$1" in
        --pid)
            [ "$#" -ge 2 ] || { usage; exit 2; }
            pid=$2
            shift 2
            ;;
        --state)
            [ "$#" -ge 2 ] || { usage; exit 2; }
            state=$2
            shift 2
            ;;
        --framebuffer)
            [ "$#" -ge 2 ] || { usage; exit 2; }
            framebuffer=$2
            shift 2
            ;;
        --artifact)
            [ "$#" -ge 2 ] || { usage; exit 2; }
            artifact=$2
            shift 2
            ;;
        --timeout)
            [ "$#" -ge 2 ] || { usage; exit 2; }
            timeout_seconds=$2
            shift 2
            ;;
        --help|-h)
            usage
            exit 0
            ;;
        *)
            usage
            exit 2
            ;;
    esac
done

[ -n "$pid" ] && [ -n "$state" ] && [ -n "$framebuffer" ] || {
    usage
    exit 2
}

case "$pid" in
    *[!0-9]*|'') printf '%s\n' 'capture-viewport-runtime: PID must be numeric' >&2; exit 2 ;;
esac
case "$timeout_seconds" in
    *[!0-9]*|'') printf '%s\n' 'capture-viewport-runtime: timeout must be an integer' >&2; exit 2 ;;
esac

[ -d "/proc/$pid" ] || {
    printf 'capture-viewport-runtime: compositor PID %s is not running\n' "$pid" >&2
    exit 2
}

script_dir=$(CDPATH= cd "$(dirname "$0")" && pwd)
repo_root=$(CDPATH= cd "$script_dir/.." && pwd)
cd "$repo_root"
command -v python3 >/dev/null 2>&1 || {
    printf '%s\n' 'capture-viewport-runtime: python3 is required' >&2
    exit 2
}

python3 - "$pid" "$state" "$framebuffer" "$timeout_seconds" <<'PY'
import os
import signal
import sys
import time
from pathlib import Path

pid = int(sys.argv[1])
state = Path(sys.argv[2])
framebuffer = Path(sys.argv[3])
timeout = int(sys.argv[4])

def signature(path: Path):
    try:
        stat = path.stat()
    except FileNotFoundError:
        return None
    return (stat.st_mtime_ns, stat.st_size)

before_state = signature(state)
before_frame = signature(framebuffer)
os.kill(pid, signal.SIGUSR1)
deadline = time.monotonic() + timeout
while time.monotonic() < deadline:
    after_state = signature(state)
    after_frame = signature(framebuffer)
    if (
        after_state is not None
        and after_frame is not None
        and after_state != before_state
        and after_frame != before_frame
        and after_frame[1] > 0
    ):
        break
    time.sleep(0.05)
else:
    raise SystemExit(
        "capture-viewport-runtime: timed out waiting for a new compositor state/frame pair"
    )
PY

if [ -n "$artifact" ]; then
    exec "$script_dir/verify-viewport-runtime.sh" \
        --state "$state" --framebuffer "$framebuffer" --artifact "$artifact"
fi
exec "$script_dir/verify-viewport-runtime.sh" \
    --state "$state" --framebuffer "$framebuffer"

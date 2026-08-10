#!/bin/sh
# Copyright (c) 2026 Palaash Atri
# SPDX-License-Identifier: MIT

# Linux-only runtime driver for the compositor-owned viewport gate. The
# compositor must have been started with absolute SLOPOS_SHOT_PATH and
# SLOPOS_SESSION_RUNTIME_DIR values. This helper proves that the target PID is
# the compositor, sends its SIGUSR1 readback request, waits for a new state /
# framebuffer pair with a completed layer configure handshake, then invokes the
# strict JSON/PNG validator. It never captures the host window.

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
        'STATE and PNG must be absolute; the compositor must expose matching absolute SLOPOS_SHOT_PATH and SLOPOS_SESSION_RUNTIME_DIR values.' >&2
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

python3 - "$pid" "$state" "$framebuffer" "$timeout_seconds" "$artifact" <<'PY'
import json
import os
import signal
import sys
import time
from pathlib import Path

pid = int(sys.argv[1])
state = Path(sys.argv[2])
framebuffer = Path(sys.argv[3])
timeout = int(sys.argv[4])
artifact_value = sys.argv[5]
try:
    artifact = Path(artifact_value).resolve(strict=False) if artifact_value else None
except OSError:
    artifact = None

def fail(message: str):
    if artifact is not None:
        evidence = {
            "schema_version": 1,
            "component": "slopos-viewport-gate",
            "status": "failed",
            "evidence_level": "runtime_capture_driver",
            "state_path": str(state),
            "framebuffer_path": str(framebuffer),
            "commit": None,
            "branch": None,
            "backend": None,
            "output": None,
            "framebuffer": None,
            "layers": [],
            "checks": {},
            "failures": [message],
            "capture_driver": {
                "pid": pid,
                "observed_at_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
            },
        }
        try:
            artifact.parent.mkdir(parents=True, exist_ok=True)
            temporary = artifact.with_name(f".{artifact.name}.tmp-{pid}")
            temporary.write_text(json.dumps(evidence, indent=2, sort_keys=True) + "\n", encoding="utf-8")
            os.replace(temporary, artifact)
        except OSError:
            pass
    raise SystemExit(f"capture-viewport-runtime: {message}")

def absolute(path: Path, label: str) -> Path:
    if not path.is_absolute():
        fail(f"{label} must be an absolute path")
    try:
        return path.resolve(strict=False)
    except OSError as error:
        fail(f"cannot resolve {label}: {error}")

state = absolute(state, "--state")
framebuffer = absolute(framebuffer, "--framebuffer")
if timeout <= 0:
    fail("timeout must be greater than zero")

proc = Path("/proc") / str(pid)
if not proc.is_dir():
    fail(f"compositor PID {pid} is not running")

def read_proc(path: Path, label: str, binary: bool = False):
    try:
        raw = path.read_bytes()
    except OSError as error:
        fail(f"cannot read {label}: {error}")
    if binary:
        return raw
    return raw.rstrip(b"\0").decode("utf-8", "surrogateescape")

def process_identity():
    comm = read_proc(proc / "comm", "compositor process name").strip()
    cmdline = read_proc(proc / "cmdline", "compositor command line", binary=True)
    argv0 = cmdline.split(b"\0", 1)[0].decode("utf-8", "surrogateescape")
    stat = read_proc(proc / "stat", "compositor process stat")
    closing_paren = stat.rfind(")")
    stat_fields = stat[closing_paren + 2 :].split() if closing_paren >= 0 else []
    if len(stat_fields) <= 19:
        fail("cannot determine compositor PID start time")
    start_time = stat_fields[19]
    try:
        executable = os.readlink(proc / "exe")
    except OSError as error:
        fail(f"cannot resolve compositor executable: {error}")
    executable_name = Path(executable.removesuffix(" (deleted)")).name
    if executable_name != "slopos-compositor":
        fail(
            "PID executable is not slopos-compositor "
            f"(comm={comm!r}, argv0={argv0!r}, exe={executable!r})"
        )
    return (comm, argv0, executable, start_time)

identity = process_identity()

def process_environment():
    raw = read_proc(proc / "environ", "compositor environment", binary=True)
    values = {}
    for item in raw.split(b"\0"):
        if b"=" not in item:
            continue
        key, value = item.split(b"=", 1)
        values[key.decode("utf-8", "surrogateescape")] = value.decode(
            "utf-8", "surrogateescape"
        )
    return values

environment = process_environment()
shot_value = environment.get("SLOPOS_SHOT_PATH")
runtime_value = environment.get("SLOPOS_SESSION_RUNTIME_DIR")
if not shot_value:
    fail("compositor has no explicit SLOPOS_SHOT_PATH; host/default capture is refused")
if not runtime_value:
    fail("compositor has no SLOPOS_SESSION_RUNTIME_DIR")
shot_path = absolute(Path(shot_value), "compositor SLOPOS_SHOT_PATH")
runtime_dir = absolute(Path(runtime_value), "compositor SLOPOS_SESSION_RUNTIME_DIR")
if shot_path != framebuffer:
    fail(
        "--framebuffer does not match compositor SLOPOS_SHOT_PATH "
        f"({framebuffer} != {shot_path})"
    )
expected_state = (runtime_dir / "viewport-state.json").resolve(strict=False)
if state != expected_state:
    fail(
        "--state does not match compositor viewport-state.json "
        f"({state} != {expected_state})"
    )

def signature(path: Path):
    try:
        stat = path.stat()
    except OSError:
        return None
    return (stat.st_dev, stat.st_ino, stat.st_mtime_ns, stat.st_size)

def load_state():
    try:
        raw = state.read_bytes()
        value = json.loads(raw.decode("utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError):
        return None
    return value if isinstance(value, dict) else None

def frame_revision(value):
    if not isinstance(value, dict):
        return None
    output = value.get("output")
    if not isinstance(output, dict):
        return None
    revision = output.get("frame_revision")
    return revision if isinstance(revision, int) and not isinstance(revision, bool) else None

def declared_frame(value):
    if not isinstance(value, dict):
        return None
    framebuffer_value = value.get("framebuffer")
    if not isinstance(framebuffer_value, dict):
        return None
    declared = framebuffer_value.get("path")
    if not isinstance(declared, str) or not declared:
        return None
    declared_path = Path(declared)
    if not declared_path.is_absolute():
        declared_path = state.parent / declared_path
    try:
        return declared_path.resolve(strict=False)
    except OSError:
        return None

def layer_handshake_complete(value):
    if not isinstance(value, dict):
        return False
    provenance = value.get("provenance")
    if not isinstance(provenance, dict):
        return False
    if provenance.get("kind") != "runtime" or provenance.get("capture") != "compositor_framebuffer":
        return False
    revision = frame_revision(value)
    if revision is None or revision < 1:
        return False
    layers = value.get("layers")
    if not isinstance(layers, list):
        return False
    by_role = {
        layer.get("role"): laye
        for layer in layers
        if isinstance(layer, dict) and isinstance(layer.get("role"), str)
    }
    for role in ("background", "menu", "dock"):
        layer = by_role.get(role)
        if not isinstance(layer, dict):
            return False
        if layer.get("acknowledged") is not True or layer.get("committed") is not True:
            return False
        configure_serial = layer.get("configure_serial")
        ack_serial = layer.get("ack_serial")
        committed_revision = layer.get("committed_frame_revision")
        if not isinstance(configure_serial, int) or configure_serial < 1:
            return False
        if ack_serial != configure_serial or committed_revision != revision:
            return False
    return True

before_state = signature(state)
before_frame = signature(framebuffer)
before_value = load_state()
before_revision = frame_revision(before_value)
try:
    os.kill(pid, signal.SIGUSR1)
except OSError as error:
    fail(f"could not send SIGUSR1 to compositor PID {pid}: {error}")
deadline = time.monotonic() + timeout
while time.monotonic() < deadline:
    if process_identity() != identity:
        fail("compositor PID identity changed while waiting for capture")
    after_state = signature(state)
    after_frame = signature(framebuffer)
    after_value = load_state()
    after_revision = frame_revision(after_value)
    if (
        after_state is not None
        and after_frame is not None
        and after_state != before_state
        and after_frame != before_frame
        and after_frame[3] > 0
        and after_revision is not None
        and after_revision > (before_revision or 0)
        and declared_frame(after_value) == framebuffe
        and layer_handshake_complete(after_value)
    ):
        print(
            "capture-viewport-runtime: compositor capture handshake observed "
            f"frame_revision={after_revision}"
        )
        break
    time.sleep(0.05)
else:
    fail(
        "timed out waiting for a new compositor state/frame pair with "
        "an acknowledged, committed background/menu/dock handshake"
    )
PY

if [ -n "$artifact" ]; then
    exec "$script_dir/verify-viewport-runtime.sh" \
        --state "$state" --framebuffer "$framebuffer" --artifact "$artifact"
fi
exec "$script_dir/verify-viewport-runtime.sh" \
    --state "$state" --framebuffer "$framebuffer"

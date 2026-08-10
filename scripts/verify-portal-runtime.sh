#!/bin/sh
# Copyright (c) 2026 Palaash Atri
# SPDX-License-Identifier: MIT

# Linux-only runtime probe for the standard portal frontend. This does not
# claim a working chooser, URI launcher, permission store, or PipeWire graph.

set -eu

case "$(uname -s)" in
    Linux) ;;
    *)
        printf '%s\n' 'verify-portal-runtime: Linux is required' >&2
        exit 2
        ;;
esac

script_dir=$(CDPATH= cd "$(dirname "$0")" && pwd)
repo_root=$(CDPATH= cd "$script_dir/.." && pwd)
cd "$repo_root"

for tool in cargo dbus-run-session git python3; do
    command -v "$tool" >/dev/null 2>&1 || {
        printf 'verify-portal-runtime: missing required tool: %s\n' "$tool" >&2
        exit 2
    }
done

target_dir=${CARGO_TARGET_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/slopos-i/cargo-target}
export CARGO_TARGET_DIR="$target_dir"
artifact_dir=${SLOPOS_QA_ARTIFACT_DIR:-artifacts/qa/portal-runtime}
mkdir -p "$artifact_dir"
commit=${SLOPOS_COMMIT:-}
if [ -z "$commit" ]; then
    if ! commit=$(git rev-parse HEAD 2>/dev/null); then
        printf '%s\n' 'verify-portal-runtime: set SLOPOS_COMMIT outside a Git checkout' >&2
        exit 2
    fi
fi
branch=${GIT_BRANCH:-}
if [ -z "$branch" ]; then
    branch=$(git branch --show-current 2>/dev/null || true)
fi
export GIT_BRANCH=${branch:-unknown}
log="$artifact_dir/${commit}-build.log"
probe_log="$artifact_dir/${commit}-probe.log"
artifact="$artifact_dir/${commit}.json"

if ! cargo build -p slopos-shell --bin slopos-portal-smoke --locked >"$log" 2>&1; then
    printf '%s\n' 'verify-portal-runtime: portal probe build failed' >&2
    cat "$log" >&2
    exit 1
fi

if ! dbus-run-session -- "$CARGO_TARGET_DIR/debug/slopos-portal-smoke" >"$probe_log" 2>&1; then
    printf '%s\n' 'verify-portal-runtime: portal session-bus probe failed' >&2
    cat "$probe_log" >&2
    exit 1
fi

python3 - "$probe_log" "$artifact" "$commit" "$target_dir" <<'PY'
import json
import os
import sys
import tempfile
from pathlib import Path

probe_path = Path(sys.argv[1])
artifact_path = Path(sys.argv[2])
commit = sys.argv[3]
target_dir = sys.argv[4]
lines = [line.strip() for line in probe_path.read_text(encoding="utf-8").splitlines()]
if not lines:
    raise SystemExit("portal probe produced no JSON result")
try:
    result = json.loads(lines[-1])
except json.JSONDecodeError as error:
    raise SystemExit(f"portal probe result is not JSON: {error}") from error
if result.get("status") != "passed":
    raise SystemExit("portal probe did not report status=passed")
payload = {
    "schema_version": 1,
    "status": "passed",
    "commit": commit,
    "branch": os.environ.get("GIT_BRANCH", "unknown"),
    "command": "dbus-run-session -- target/debug/slopos-portal-smoke",
    "target_dir": str(target_dir),
    "probe": result,
}
artifact_path.parent.mkdir(parents=True, exist_ok=True)
fd, temporary = tempfile.mkstemp(prefix=f".{artifact_path.name}.", dir=artifact_path.parent)
try:
    with os.fdopen(fd, "w", encoding="utf-8") as stream:
        json.dump(payload, stream, indent=2, sort_keys=True)
        stream.write("\n")
        stream.flush()
        os.fsync(stream.fileno())
    os.replace(temporary, artifact_path)
finally:
    if os.path.exists(temporary):
        os.unlink(temporary)
PY

cat "$probe_log"
printf 'verify-portal-runtime: passed (frontend registration only; live backends remain unimplemented)\n'

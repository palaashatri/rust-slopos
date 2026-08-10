#!/bin/sh
# Copyright (c) 2026 Palaash Atri
# SPDX-License-Identifier: MIT

# Linux-only compositor viewport correctness gate.
#
# Normal mode requires a compositor-owned, schema-versioned state JSON and the
# exact PNG framebuffer named by that state.  The gate does not use host-window
# screenshots or infer dimensions from a display manager.  Use --self-test for
# the deterministic positive/negative fixture path.

set -eu

case "$(uname -s)" in
    Linux)
        ;;
    *)
        printf '%s\n' 'verify-viewport-runtime: Linux is required' >&2
        exit 2
        ;;
esac

script_dir=$(CDPATH= cd "$(dirname "$0")" && pwd)
repo_root=$(CDPATH= cd "$script_dir/.." && pwd)
cd "$repo_root"

command -v python3 >/dev/null 2>&1 || {
    printf '%s\n' 'verify-viewport-runtime: python3 is required' >&2
    exit 2
}

exec python3 "$script_dir/viewport_gate.py" "$@"

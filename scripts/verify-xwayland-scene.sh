#!/usr/bin/env bash
# Copyright (c) 2026 Palaash Atri
# SPDX-License-Identifier: MIT

# Real X11-client lifecycle evidence for the bounded rootless XWayland scene.
# Headless proves protocol/scene lifecycle only; it never claims pixels,
# physical input, DRM/KMS, or broad third-party compatibility.

set -euo pipefail

if [[ "$(uname -s)" != "Linux" ]]; then
    printf '%s\n' 'verify-xwayland-scene: Linux is required' >&2
    exit 2
fi

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"
CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-${XDG_CACHE_HOME:-$HOME/.cache}/slopos-i/cargo-target}"
export CARGO_TARGET_DIR
mkdir -p "$CARGO_TARGET_DIR"
for tool in cargo git grep awk ps xmessage xwininfo xprop Xvfb mktemp; do
    command -v "$tool" >/dev/null 2>&1 || {
        printf 'verify-xwayland-scene: missing required tool: %s\n' "$tool" >&2
        exit 2
    }
done

commit_sha="$(git rev-parse HEAD)"
branch="$(git branch --show-current || true)"
artifact_dir="$(printenv SLOPOS_QA_ARTIFACT_DIR || printf '%s' artifacts/qa/xwayland-scene)"
mkdir -p "$artifact_dir"
log="$artifact_dir/$commit_sha-compositor.log"
build_log="$artifact_dir/$commit_sha-build.log"
client_log="$artifact_dir/$commit_sha-xmessage.log"
xwininfo_log="$artifact_dir/$commit_sha-xwininfo-root-tree.log"
xprop_log="$artifact_dir/$commit_sha-xprop.log"
nested_log="$artifact_dir/$commit_sha-nested.log"
pre_log="$artifact_dir/$commit_sha-pre-ready.log"
result="$artifact_dir/$commit_sha.json"
provenance="$artifact_dir/$commit_sha-provenance.txt"

headless_pid=""
xmessage_pid=""
nested_pid=""
xvfb_pid=""
pre_pid=""
runtime=""
nested_runtime=""
pre_runtime=""
status=failed
failure=not_started
scene_mapping_verified=false
scene_configure_verified=false
scene_rendered_verified=false
scene_hit_test_verified=false
keyboard_focus_selected_verified=false
scene_unmap_destroy_verified=false
compositor_survived_client_exit=false
startup_watchdog_verified=false
nested_dri3_available=false

count_marker() {
    grep -c "$1" "$2" 2>/dev/null || true
}

wait_for_log() {
    for _ in $(seq 1 120); do
        grep -q "$1" "$2" 2>/dev/null && return 0
        sleep 0.1
    done
    return 1
}

xwayland_child_pid() {
    ps -o pid=,args= --ppid "$1" |
        awk '$2 ~ /^Xwayland([[:space:]]|$)/ { print $1; exit }'
}

stop_process() {
    [[ -z "$1" ]] && return 0
    kill -TERM "$1" 2>/dev/null || true
    for _ in $(seq 1 30); do
        kill -0 "$1" 2>/dev/null || return 0
        sleep 0.1
    done
    kill -KILL "$1" 2>/dev/null || true
    wait "$1" 2>/dev/null || true
}

write_result() {
    cat >"$result" <<JSON
{
  "schema": 1,
  "component": "slopos-compositor-xwayland-scene",
  "commit": "$commit_sha",
  "branch": "$branch",
  "status": "$status",
  "failure": "$failure",
  "evidence_level": "headless_xwayland_scene_runtime",
  "scene_mapping_verified": $scene_mapping_verified,
  "scene_configure_verified": $scene_configure_verified,
  "scene_rendered_verified": $scene_rendered_verified,
  "scene_hit_test_verified": $scene_hit_test_verified,
  "keyboard_focus_selected_verified": $keyboard_focus_selected_verified,
  "scene_unmap_destroy_verified": $scene_unmap_destroy_verified,
  "compositor_survived_client_exit": $compositor_survived_client_exit,
  "xwayland_startup_watchdog_verified": $startup_watchdog_verified,
  "nested_dri3_available": $nested_dri3_available,
  "rendering_verified": false,
  "physical_input_verified": false,
  "drm_verified": false,
  "hardware_verified": false,
  "broad_x11_compatibility_verified": false
}
JSON
}

cleanup() {
    code=$?
    [[ -z "$xmessage_pid" ]] || stop_process "$xmessage_pid"
    if [[ -n "$headless_pid" ]]; then
        child="$(xwayland_child_pid "$headless_pid" || true)"
        [[ -z "$child" ]] || kill -KILL "$child" 2>/dev/null || true
        stop_process "$headless_pid"
    fi
    if [[ -n "$nested_pid" ]]; then
        child="$(xwayland_child_pid "$nested_pid" || true)"
        [[ -z "$child" ]] || kill -KILL "$child" 2>/dev/null || true
        stop_process "$nested_pid"
    fi
    [[ -z "$xvfb_pid" ]] || stop_process "$xvfb_pid"
    if [[ -n "$pre_pid" ]]; then
        child="$(xwayland_child_pid "$pre_pid" || true)"
        [[ -z "$child" ]] || kill -KILL "$child" 2>/dev/null || true
        stop_process "$pre_pid"
    fi
    [[ -z "$runtime" ]] || rm -rf "$runtime"
    [[ -z "$nested_runtime" ]] || rm -rf "$nested_runtime"
    [[ -z "$pre_runtime" ]] || rm -rf "$pre_runtime"
    write_result
    exit "$code"
}
trap cleanup EXIT INT TERM

cat >"$provenance" <<EOF
commit=$commit_sha
branch=$branch
host=$(hostname)
uname=$(uname -a)
command=SLOPOS_QA_ARTIFACT_DIR=$artifact_dir scripts/verify-xwayland-scene.sh
EOF
git diff -- crates/slopos-compositor/src/main.rs \
    crates/slopos-compositor/tests/compositor_completion_contract.rs \
    scripts/verify-xwayland-scene.sh >"$artifact_dir/$commit_sha-working-tree.diff" || true

cargo build -p slopos-compositor --bin slopos-compositor --release --locked >"$build_log" 2>&1

runtime="$(mktemp -d /tmp/slopos-xwayland-scene.XXXXXX)"
chmod 700 "$runtime"
export XDG_RUNTIME_DIR="$runtime"
export SLOPOS_SESSION_RUNTIME_DIR="$runtime"
export SLOPOS_SESSION_TOKEN="xwayland-scene-$commit_sha-$$"
unset DISPLAY WAYLAND_DISPLAY SLOPOS_CLIENT_WAYLAND_DISPLAY SLOPOS_XWAYLAND_DISPLAY

"$CARGO_TARGET_DIR/release/slopos-compositor" --backend headless >"$log" 2>&1 &
headless_pid=$!
wait_for_log 'XWayland ready on DISPLAY=' "$log" || {
    failure=headless_xwayland_ready_timeout
    exit 1
}
xdisplay="$(grep -o 'XWayland ready on DISPLAY=:[0-9]*' "$log" | tail -1 | sed 's/.*=//')"
[[ -n "$xdisplay" ]] || {
    failure=xwayland_display_not_found
    exit 1
}

DISPLAY="$xdisplay" xmessage -geometry 320x180+100+100 \
    -title SLOPOS-X11-SCENE 'SLOPOS XWayland scene smoke' >"$client_log" 2>&1 &
xmessage_pid=$!
wait_for_log 'XWayland surface mapped into scene' "$log" || {
    failure=scene_mapping_timeout
    exit 1
}
if grep -q 'XWayland surface associated with wl_surface' "$log" &&
    grep -q 'X11 map_window_request granted' "$log"; then
    scene_mapping_verified=true
fi
DISPLAY="$xdisplay" xwininfo -root -tree >"$xwininfo_log" 2>&1 || true
DISPLAY="$xdisplay" xprop -root _NET_CLIENT_LIST _NET_ACTIVE_WINDOW >"$xprop_log" 2>&1 || true
if grep -q '320x180+100+100' "$xwininfo_log" &&
    grep -q 'SLOPOS-X11-SCENE' "$xwininfo_log"; then
    scene_configure_verified=true
fi
grep -q 'XWayland surface configured in scene' "$log" && scene_configure_verified=true || true
grep -q 'XWayland surface rendered' "$log" && scene_rendered_verified=true || true
grep -q 'XWayland surface hit-tested for input' "$log" && scene_hit_test_verified=true || true
grep -q 'XWayland keyboard focus selected' "$log" &&
    keyboard_focus_selected_verified=true || true

kill -TERM "$xmessage_pid" 2>/dev/null || true
wait "$xmessage_pid" 2>/dev/null || true
xmessage_pid=""
if wait_for_log 'XWayland surface destroyed from scene' "$log" &&
    grep -q 'XWayland surface unmapped from scene' "$log"; then
    scene_unmap_destroy_verified=true
fi
kill -0 "$headless_pid" 2>/dev/null && compositor_survived_client_exit=true || true

# Retain an explicit nested capability result. Xvfb without DRI3 is a
# capability block, not a rendering pass.
export DISPLAY=:101
Xvfb :101 -screen 0 1024x768x24 -nolisten tcp >"$artifact_dir/$commit_sha-xvfb.log" 2>&1 &
xvfb_pid=$!
sleep 0.3
nested_runtime="$(mktemp -d /tmp/slopos-xwayland-nested.XXXXXX)"
chmod 700 "$nested_runtime"
XDG_RUNTIME_DIR="$nested_runtime" SLOPOS_SESSION_RUNTIME_DIR="$nested_runtime" \
    SLOPOS_SESSION_TOKEN="xwayland-nested-$commit_sha-$$" LIBGL_ALWAYS_SOFTWARE=1 \
    GALLIUM_DRIVER=llvmpipe "$CARGO_TARGET_DIR/release/slopos-compositor" --backend nested \
    >"$nested_log" 2>&1 &
nested_pid=$!
sleep 1
if kill -0 "$nested_pid" 2>/dev/null && [[ -s "$nested_runtime/readiness" ]]; then
    nested_dri3_available=true
fi

# Smithay maps displayfd EOF to a quiet event when XWayland dies before Ready.
# The compositor watchdog observes the XWayland Wayland client disappearing.
pre_runtime="$(mktemp -d /tmp/slopos-xwayland-pre.XXXXXX)"
chmod 700 "$pre_runtime"
XDG_RUNTIME_DIR="$pre_runtime" SLOPOS_SESSION_RUNTIME_DIR="$pre_runtime" \
    SLOPOS_SESSION_TOKEN="xwayland-pre-ready-$commit_sha-$$" \
    "$CARGO_TARGET_DIR/release/slopos-compositor" --backend headless >"$pre_log" 2>&1 &
pre_pid=$!
pre_child=""
for _ in $(seq 1 200); do
    pre_child="$(xwayland_child_pid "$pre_pid" || true)"
    [[ -n "$pre_child" ]] && break
    sleep 0.01
done
if [[ -n "$pre_child" ]] && ! grep -q 'XWayland ready on DISPLAY=' "$pre_log"; then
    kill -KILL "$pre_child" 2>/dev/null || true
fi
if wait_for_log 'XWayland startup failed; restarting' "$pre_log" &&
    kill -0 "$pre_pid" 2>/dev/null &&
    [[ "$(count_marker 'XWayland spawning' "$pre_log")" -ge 2 ]]; then
    startup_watchdog_verified=true
fi

if [[ "$scene_mapping_verified" = true &&
    "$scene_unmap_destroy_verified" = true &&
    "$startup_watchdog_verified" = true ]]; then
    status=passed
    failure=""
else
    failure=one_or_more_headless_scene_or_watchdog_checks_failed
    exit 1
fi
printf 'XWayland scene lifecycle and xwayland-startup-watchdog verified at %s\n' \
    "$commit_sha"

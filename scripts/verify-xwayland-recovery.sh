#!/usr/bin/env bash
# Copyright (c) 2026 Palaash Atri
# SPDX-License-Identifier: MIT

# Linux-only runtime gate for the bounded XWayland crash-recovery path.
# This intentionally uses the SLOPOS-owned headless backend. It proves that a
# ready XWayland child can die without taking down the compositor, that the
# compositor starts a replacement for each permitted recovery, and that the
# session-scoped budget stops a crash loop. It does not prove physical DRM,
# X11 application compatibility, input hardware, rendering or accessibility.

set -euo pipefail

if [[ "$(uname -s)" != "Linux" ]]; then
    printf 'verify-xwayland-recovery: Linux is required\n' >&2
    exit 2
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

for tool in cargo git grep awk ps pgrep mktemp; do
    if ! command -v "$tool" >/dev/null 2>&1; then
        printf 'verify-xwayland-recovery: missing required tool: %s\n' "$tool" >&2
        exit 2
    fi
done

commit_sha="$(git rev-parse HEAD)"
branch="$(git branch --show-current || true)"
timestamp="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
artifact_dir="${SLOPOS_QA_ARTIFACT_DIR:-artifacts/qa/xwayland-recovery}"
mkdir -p "$artifact_dir"
log="$artifact_dir/${commit_sha}-compositor.log"
build_log="$artifact_dir/${commit_sha}-build.log"
result="$artifact_dir/${commit_sha}.json"
provenance="$artifact_dir/${commit_sha}-provenance.txt"
runtime_dir="$(mktemp -d "${TMPDIR:-/tmp}/slopos-xwayland-recovery.XXXXXX")"
chmod 700 "$runtime_dir"

compositor_pid=""
status="failed"
failure="not_started"
ready_count=0
disconnected_count=0
replacement_ready_verified=false
budget_exhaustion_verified=false
compositor_survived_verified=false

write_result() {
    cat >"$result" <<JSON
{
  "schema": 1,
  "component": "slopos-compositor-xwayland-recovery",
  "commit": "$commit_sha",
  "branch": "$branch",
  "started_at_utc": "$timestamp",
  "status": "$status",
  "failure": "$failure",
  "evidence_level": "headless_runtime_recovery",
  "backend": "headless",
  "restart_budget": 3,
  "xwayland_ready_count": $ready_count,
  "xwayland_disconnected_count": $disconnected_count,
  "replacement_ready_verified": $replacement_ready_verified,
  "budget_exhaustion_verified": $budget_exhaustion_verified,
  "compositor_survived_verified": $compositor_survived_verified,
  "hardware_verified": false,
  "drm_verified": false,
  "physical_input_verified": false,
  "third_party_x11_application_verified": false
}
JSON
}

cleanup() {
    exit_code=$?
    if [[ -n "$compositor_pid" ]] && kill -0 "$compositor_pid" 2>/dev/null; then
        kill -TERM "$compositor_pid" 2>/dev/null || true
        for _ in $(seq 1 50); do
            if ! kill -0 "$compositor_pid" 2>/dev/null; then
                break
            fi
            sleep 0.1
        done
        kill -KILL "$compositor_pid" 2>/dev/null || true
        wait "$compositor_pid" 2>/dev/null || true
    fi
    ready_count="$(grep -c 'XWayland ready on DISPLAY=' "$log" 2>/dev/null || true)"
    disconnected_count="$(grep -c 'XWayland WM disconnected$' "$log" 2>/dev/null || true)"
    write_result
    rm -rf "$runtime_dir"
    exit "$exit_code"
}
trap cleanup EXIT INT TERM

cat >"$provenance" <<EOF
commit=$commit_sha
branch=$branch
timestamp_utc=$timestamp
host=$(hostname)
uname=$(uname -a)
command=SLOPOS_QA_ARTIFACT_DIR=$artifact_dir scripts/verify-xwayland-recovery.sh
EOF

printf 'Building exact-commit compositor %s\n' "$commit_sha"
if ! cargo build -p slopos-compositor --bin slopos-compositor --release --locked >"$build_log" 2>&1; then
    failure="release_build_failed"
    cat "$build_log" >&2
    exit 1
fi

export XDG_RUNTIME_DIR="$runtime_dir"
export SLOPOS_SESSION_RUNTIME_DIR="$runtime_dir"
export SLOPOS_SESSION_TOKEN="xwayland-recovery-${commit_sha}-$$"
unset DISPLAY WAYLAND_DISPLAY SLOPOS_CLIENT_WAYLAND_DISPLAY SLOPOS_XWAYLAND_DISPLAY

printf 'Starting SLOPOS-owned headless compositor\n'
target/release/slopos-compositor --backend headless >"$log" 2>&1 &
compositor_pid=$!

for _ in $(seq 1 100); do
    if [[ -s "$runtime_dir/readiness" ]]; then
        break
    fi
    if ! kill -0 "$compositor_pid" 2>/dev/null; then
        failure="compositor_exited_before_readiness"
        exit 1
    fi
    sleep 0.1
done
if [[ ! -s "$runtime_dir/readiness" ]]; then
    failure="readiness_timeout"
    exit 1
fi

for _ in $(seq 1 100); do
    if grep -q 'XWayland ready on DISPLAY=' "$log"; then
        break
    fi
    if ! kill -0 "$compositor_pid" 2>/dev/null; then
        failure="compositor_exited_before_xwayland_ready"
        exit 1
    fi
    sleep 0.1
done
if ! grep -q 'XWayland ready on DISPLAY=' "$log"; then
    failure="xwayland_ready_timeout"
    exit 1
fi

xwayland_child_pid() {
    ps -o pid=,args= --ppid "$compositor_pid" |
        awk '$2 ~ /^Xwayland([[:space:]]|$)/ { print $1; exit }'
}

wait_for_count() {
    pattern="$1"
    expected="$2"
    for _ in $(seq 1 100); do
        count="$(grep -c "$pattern" "$log" 2>/dev/null || true)"
        if [[ "$count" -ge "$expected" ]]; then
            return 0
        fi
        if ! kill -0 "$compositor_pid" 2>/dev/null; then
            return 1
        fi
        sleep 0.1
    done
    return 1
}

for attempt in 1 2 3 4; do
    xwayland_pid="$(xwayland_child_pid || true)"
    if [[ -z "$xwayland_pid" ]]; then
        failure="xwayland_child_missing_before_kill_${attempt}"
        exit 1
    fi
    kill -KILL "$xwayland_pid"

    if ! wait_for_count 'XWayland WM disconnected$' "$attempt"; then
        failure="disconnect_not_observed_${attempt}"
        exit 1
    fi
    if ! kill -0 "$compositor_pid" 2>/dev/null; then
        failure="compositor_died_after_xwayland_disconnect_${attempt}"
        exit 1
    fi
    compositor_survived_verified=true

    if (( attempt <= 3 )); then
        if ! wait_for_count 'XWayland ready on DISPLAY=' "$((attempt + 1))"; then
            failure="replacement_not_ready_${attempt}"
            exit 1
        fi
        replacement_ready_verified=true
    else
        if ! wait_for_count 'XWayland recovery budget exhausted' 1; then
            failure="budget_exhaustion_not_observed"
            exit 1
        fi
        budget_exhaustion_verified=true
        sleep 0.5
        if [[ "$(grep -c 'XWayland ready on DISPLAY=' "$log" 2>/dev/null || true)" -ne 4 ]]; then
            failure="restart_occurred_after_budget_exhaustion"
            exit 1
        fi
    fi
done

status="passed"
failure=""
printf 'XWayland bounded recovery verified at %s\n' "$commit_sha"

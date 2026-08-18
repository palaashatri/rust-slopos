#!/usr/bin/env bash
# Measure SLOPOS-I X11 session startup and resident memory in a disposable display.
set -euo pipefail

export DISPLAY="${DISPLAY:-:99}"
export SLOPOS_OPENBOX_CONFIG="${SLOPOS_OPENBOX_CONFIG:-$PWD/assets/config/openbox/rc.xml}"
export SLOPOS_QA_NO_WELCOME=1

for command_name in Xvfb dbus-run-session pgrep ps xdotool; do
  command -v "$command_name" >/dev/null || {
    echo "missing required command: $command_name" >&2
    exit 2
  }
done

hold_seconds="${SLOPOS_BENCHMARK_HOLD_SECONDS:-30}"
max_growth_kib="${SLOPOS_BENCHMARK_MAX_RSS_GROWTH_KIB:-32768}"
[[ "$hold_seconds" =~ ^[0-9]+$ ]] || {
  echo "SLOPOS_BENCHMARK_HOLD_SECONDS must be a non-negative integer" >&2
  exit 2
}
[[ "$max_growth_kib" =~ ^[0-9]+$ ]] || {
  echo "SLOPOS_BENCHMARK_MAX_RSS_GROWTH_KIB must be a non-negative integer" >&2
  exit 2
}

test -x target/release/slopos-session
test -x target/release/slopos-shell

runtime_dir="${XDG_RUNTIME_DIR:-/tmp/slopos-benchmark-runtime}"
mkdir -p "$runtime_dir"
chmod 700 "$runtime_dir"

cleanup() {
  set +e
  terminate_pid "${session_child_pid:-}"
  terminate_pid "${SESSION_PID:-}"
  terminate_pid "${shell_pid:-}"
  terminate_pid "${openbox_pid:-}"
  for process in slopos-shell openbox; do
    for _ in $(seq 1 20); do
      [[ -z "$(live_pid "$process")" ]] && break
      sleep 0.1
    done
  done
  terminate_pid "${XVFB_PID:-}"
}
trap cleanup EXIT

terminate_pid() {
  local pid="$1"
  [[ "$pid" =~ ^[0-9]+$ ]] || return 0
  [[ "$(ps -o stat= -p "$pid" 2>/dev/null | tr -d ' ')" == Z* ]] && return 0
  kill -TERM "$pid" 2>/dev/null || true
  for _ in $(seq 1 20); do
    kill -0 "$pid" 2>/dev/null || return 0
    sleep 0.1
  done
  kill -KILL "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
}

live_pid() {
  local process="$1"
  ps -eo pid=,stat=,comm= | awk -v process="$process" \
    '$3 == process && $2 !~ /^Z/ { print $1; exit }'
}

start_ms="$(date +%s%3N)"
Xvfb "$DISPLAY" -screen 0 "${SLOPOS_BENCHMARK_SCREEN:-1280x800}x24" \
  >"$runtime_dir/xvfb.log" 2>&1 &
XVFB_PID=$!
dbus-run-session -- ./target/release/slopos-session \
  >"$runtime_dir/session.log" 2>&1 &
SESSION_PID=$!

for _ in $(seq 1 60); do
  session_child_pid="$(live_pid slopos-session)"
  openbox_pid="$(live_pid openbox)"
  shell_pid="$(live_pid slopos-shell)"
  if [[ -n "$session_child_pid" && -n "$openbox_pid" && -n "$shell_pid" ]] \
      && xdotool search --onlyvisible --name '^SLOPOS Top Bar$' >/dev/null; then
    break
  fi
  sleep 0.25
done

[[ -n "$openbox_pid" ]]
[[ -n "$shell_pid" ]]
[[ -n "$session_child_pid" ]]
end_ms="$(date +%s%3N)"

tree_rss_kib() {
  local total=0
  local value
  for pid in "$session_child_pid" "$shell_pid" "$openbox_pid"; do
    value="$(ps -o rss= -p "$pid" | tr -d ' ')"
    [[ "$value" =~ ^[0-9]+$ ]] || return 1
    total=$((total + value))
  done
  printf '%s\n' "$total"
}

topbar_visible() {
  for _ in 1 2 3; do
    if xdotool search --onlyvisible --name '^SLOPOS Top Bar$' >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.1
  done
  return 1
}

initial_rss_kib="$(tree_rss_kib)"
for ((second = 0; second < hold_seconds; second++)); do
  [[ "$(live_pid slopos-session)" == "$session_child_pid" ]]
  [[ "$(live_pid openbox)" == "$openbox_pid" ]]
  [[ "$(live_pid slopos-shell)" == "$shell_pid" ]]
  topbar_visible
  sleep 1
done
final_rss_kib="$(tree_rss_kib)"
rss_delta_kib=$((final_rss_kib - initial_rss_kib))
if ((rss_delta_kib > max_growth_kib)); then
  echo "RSS growth exceeded benchmark limit: ${rss_delta_kib} KiB > ${max_growth_kib} KiB" >&2
  exit 1
fi

printf 'SESSION_STARTUP_MS=%s\n' "$((end_ms - start_ms))"
printf 'SESSION_TREE_RSS_KIB_INITIAL=%s\n' "$initial_rss_kib"
printf 'SESSION_TREE_RSS_KIB_FINAL=%s\n' "$final_rss_kib"
printf 'SESSION_TREE_RSS_DELTA_KIB=%s\n' "$rss_delta_kib"
printf 'BENCHMARK_HOLD_SECONDS=%s\n' "$hold_seconds"
printf 'BENCHMARK_MAX_RSS_GROWTH_KIB=%s\n' "$max_growth_kib"
printf 'BENCHMARK_SCREEN=%s\n' "${SLOPOS_BENCHMARK_SCREEN:-1280x800}"
printf 'BENCHMARK_STATUS_0\n'

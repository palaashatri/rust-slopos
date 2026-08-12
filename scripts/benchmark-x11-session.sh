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

test -x target/release/slopos-session
test -x target/release/slopos-shell

runtime_dir="${XDG_RUNTIME_DIR:-/tmp/slopos-benchmark-runtime}"
mkdir -p "$runtime_dir"
chmod 700 "$runtime_dir"

cleanup() {
  set +e
  kill "${SESSION_PID:-}" "${XVFB_PID:-}" 2>/dev/null || true
  pkill -TERM -x slopos-shell 2>/dev/null || true
  pkill -TERM -x openbox 2>/dev/null || true
}
trap cleanup EXIT

start_ms="$(date +%s%3N)"
Xvfb "$DISPLAY" -screen 0 "${SLOPOS_BENCHMARK_SCREEN:-1280x800}x24" \
  >"$runtime_dir/xvfb.log" 2>&1 &
XVFB_PID=$!
dbus-run-session -- ./target/release/slopos-session \
  >"$runtime_dir/session.log" 2>&1 &
SESSION_PID=$!

for _ in $(seq 1 60); do
  if pgrep -x openbox >/dev/null && pgrep -x slopos-shell >/dev/null \
      && xdotool search --onlyvisible --name '^SLOPOS Top Bar$' >/dev/null; then
    break
  fi
  sleep 0.25
done

pgrep -x openbox >/dev/null
shell_pid="$(pgrep -xo slopos-shell)"
openbox_pid="$(pgrep -xo openbox)"
end_ms="$(date +%s%3N)"

rss_kib=0
for pid in "$SESSION_PID" "$shell_pid" "$openbox_pid"; do
  value="$(ps -o rss= -p "$pid" | tr -d ' ')"
  rss_kib=$((rss_kib + value))
done

printf 'SESSION_STARTUP_MS=%s\n' "$((end_ms - start_ms))"
printf 'SESSION_TREE_RSS_KIB=%s\n' "$rss_kib"
printf 'BENCHMARK_SCREEN=%s\n' "${SLOPOS_BENCHMARK_SCREEN:-1280x800}"
printf 'BENCHMARK_STATUS_0\n'

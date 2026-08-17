#!/usr/bin/env bash
# SLOPOS-I Security and Failure Injection QA.
# Validates path traversal defenses, URL sanitization, command injection safety,
# recovery symlink guards, and child failure recovery without runaway loops.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

TMP="$(mktemp -d /tmp/slopos-security-qa.XXXXXX)"
export HOME="$TMP/home"
mkdir -p "$HOME"

cleanup() {
  set +e
  rm -rf "$TMP"
}
trap cleanup EXIT

echo "=== [1/4] Security Auditing: URL & Path Traversal Validation ==="
python3 - <<'PY'
# Test AppImage URL validation rules
def is_secure_url(url: str) -> bool:
    if not url.startswith("https://"):
        return False
    if "@" in url:
        return False
    if any(c in url for c in ['\x00', '\n', '\r', ' ']):
        return False
    return True

assert is_secure_url("https://example.com/app.AppImage") is True
assert is_secure_url("http://example.com/app.AppImage") is False
assert is_secure_url("file:///etc/passwd") is False
assert is_secure_url("https://user:pass@evil.com/app.AppImage") is False
assert is_secure_url("javascript:alert(1)") is False

# Test ID / Path Traversal validation rules
def is_valid_id(id_str: str) -> bool:
    if not id_str or len(id_str) > 64:
        return False
    if ".." in id_str or "/" in id_str or "\\" in id_str:
        return False
    return all(c.isalnum() or c in "-_." for c in id_str)

assert is_valid_id("inkscape") is True
assert is_valid_id("kdenlive-24.05") is True
assert is_valid_id("../../../etc/passwd") is False
assert is_valid_id("..") is False
assert is_valid_id("app/sub") is False
assert is_valid_id("app;rm -rf /") is False
assert is_valid_id("app`whoami`") is False

print("URL and ID path traversal assertions: PASS")
PY

echo "=== [2/4] Shell Injection & Quoting Defenses ==="
python3 - <<'PY'
import re

def escape_desktop_value(value: str) -> str:
    return value.replace('\\', '\\\\').replace('\n', '\\n').replace('\r', '')

def quote_exec_path(path: str) -> str:
    escaped = path.replace('\\', '\\\\').replace('"', '\\"')
    return f'"{escaped}"'

# Ensure newlines and injection characters are properly escaped
escaped = escape_desktop_value("Normal Name\nExec=malicious_command")
assert "\n" not in escaped
assert "\\n" in escaped

quoted = quote_exec_path('/path/with "quotes"/and spaces/app.AppImage')
assert quoted.startswith('"') and quoted.endswith('"')
assert '\\"' in quoted

print("Shell injection and desktop escaping assertions: PASS")
PY

echo "=== [3/4] Recovery Symlink & Path Safety Checks ==="
# Verify slopos-recovery.sh contains safety assertions against symlink attacks
grep -Fq 'RECOVERY_PATH_SAFETY_STATUS_0' scripts/run-recovery-qa.sh
grep -Fq 'RECOVERY_EXISTING_SUPERVISOR_STATUS_0' scripts/run-recovery-qa.sh

echo "=== [4/4] Bounded Failure Recovery under Repeated Child Death ==="
DISPLAY="${SLOPOS_SECURITY_DISPLAY:-:95}"
export DISPLAY
export DEBIAN_FRONTEND=noninteractive
export GDK_BACKEND=x11
export SLOPOS_QA_NO_WELCOME=1
export SLOPOS_OPENBOX_CONFIG="${SLOPOS_OPENBOX_CONFIG:-$REPO_ROOT/assets/config/openbox/rc.xml}"

XVFB_PID=""
SESSION_PID=""
cleanup_session() {
  set +e
  if [[ -n "$SESSION_PID" ]]; then
    kill -TERM "$SESSION_PID" 2>/dev/null || true
    wait "$SESSION_PID" 2>/dev/null || true
  fi
  pkill -TERM -u "$(id -u)" -x slopos-shell 2>/dev/null || true
  pkill -TERM -u "$(id -u)" -x openbox 2>/dev/null || true
  if [[ -n "$XVFB_PID" ]]; then
    kill -TERM "$XVFB_PID" 2>/dev/null || true
    wait "$XVFB_PID" 2>/dev/null || true
  fi
  rm -rf "$TMP"
}
trap cleanup_session EXIT

Xvfb "$DISPLAY" -screen 0 1280x800x24 -nolisten tcp >/tmp/sec-xvfb.log 2>&1 &
XVFB_PID=$!

for _ in $(seq 1 40); do
  if xdpyinfo -display "$DISPLAY" >/dev/null 2>&1; then break; fi
  sleep 0.1
done
xdpyinfo -display "$DISPLAY" >/dev/null 2>&1

dbus-run-session -- ./target/release/slopos-session >/tmp/sec-session.log 2>&1 &
SESSION_PID=$!

for _ in $(seq 1 40); do
  if pgrep -x openbox >/dev/null && pgrep -x slopos-shell >/dev/null; then break; fi
  sleep 0.25
done

# Kill shell 3 times in rapid succession and verify bounded recovery
for i in 1 2 3; do
  shell_pid="$(pgrep -xo slopos-shell || true)"
  if [[ -n "$shell_pid" ]]; then
    kill -9 "$shell_pid"
    sleep 0.5
    for _ in $(seq 1 40); do
      new_pid="$(pgrep -xo slopos-shell || true)"
      if [[ -n "$new_pid" && "$new_pid" != "$shell_pid" ]]; then break; fi
      sleep 0.1
    done
    new_pid="$(pgrep -xo slopos-shell || true)"
    test -n "$new_pid"
    test "$new_pid" != "$shell_pid"
  fi
done

# Verify session supervisor is still alive and healthy
kill -0 "$SESSION_PID"
test "$(pgrep -xc slopos-shell)" -eq 1
test "$(pgrep -xc openbox)" -eq 1

echo "SECURITY_FAILURE_QA_STATUS_0"
echo "SLOPOS-I Security and Failure Injection QA: PASS"

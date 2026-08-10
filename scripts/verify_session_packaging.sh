#!/usr/bin/env bash
# verify_session_packaging.sh — check greeter/session packaging artifacts.
#
# Confirms packaging/*.desktop, scripts/start-slopos-i, and
# packaging/slopos-i.service exist, and that desktop entries carry
# required keys (Name, Exec, Type) with session-valid values.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

FAIL=0

ok()  { echo "OK  $*"; }
err() { echo "FAIL $*" >&2; FAIL=1; }

require_file() {
  local path="$1"
  if [ -f "$path" ]; then
    ok "$path"
  else
    err "missing required file: $path"
  fi
}

# --- required artifacts -------------------------------------------------------

require_file "scripts/start-slopos-i"
require_file "packaging/slopos-i.service"

shopt -s nullglob
DESKTOPS=(packaging/*.desktop)
shopt -u nullglob

if [ "${#DESKTOPS[@]}" -eq 0 ]; then
  err "no packaging/*.desktop files found"
else
  for f in "${DESKTOPS[@]}"; do
    require_file "$f"
  done
fi

# --- desktop key checks (mirrors validate_session_desktop in Rust) ------------

check_desktop_keys() {
  local file="$1"
  local file_fail=0
  local key

  for key in Name Exec Type DesktopNames TryExec Keywords; do
    if ! grep -qE "^${key}=" "$file"; then
      err "$file missing required key: $key"
      file_fail=1
    fi
  done

  if [ "$file_fail" -ne 0 ]; then
    return
  fi

  local name exec_val type_val desktop_names try_exec keywords
  name="$(grep -E '^Name=' "$file" | head -1 | cut -d= -f2-)"
  exec_val="$(grep -E '^Exec=' "$file" | head -1 | cut -d= -f2-)"
  type_val="$(grep -E '^Type=' "$file" | head -1 | cut -d= -f2-)"
  desktop_names="$(grep -E '^DesktopNames=' "$file" | head -1 | cut -d= -f2-)"
  try_exec="$(grep -E '^TryExec=' "$file" | head -1 | cut -d= -f2-)"
  keywords="$(grep -E '^Keywords=' "$file" | head -1 | cut -d= -f2-)"

  if [ -z "$name" ]; then
    err "$file: Name is empty"
    file_fail=1
  fi
  if [ "$type_val" != "Application" ]; then
    err "$file: Type must be Application (got '$type_val')"
    file_fail=1
  fi
  if [[ "$exec_val" != *start-slopos-i* ]]; then
    err "$file: Exec must contain start-slopos-i (got '$exec_val')"
    file_fail=1
  fi
  if [ "$desktop_names" != "SLOPOS-I" ]; then
    err "$file: DesktopNames must be SLOPOS-I (got '$desktop_names')"
    file_fail=1
  fi
  if [[ "$try_exec" != *start-slopos-i* ]]; then
    err "$file: TryExec must contain start-slopos-i (got '$try_exec')"
    file_fail=1
  fi
  if [ "$keywords" != "SLOPOS-I;Wayland;Desktop;" ]; then
    err "$file: Keywords must be SLOPOS-I;Wayland;Desktop; (got '$keywords')"
    file_fail=1
  fi

  if [ "$file_fail" -eq 0 ]; then
    ok "$file keys: Name='$name' Type='$type_val' Exec='$exec_val' DesktopNames='$desktop_names'"
  fi
}

for f in "${DESKTOPS[@]:-}"; do
  [ -f "$f" ] || continue
  check_desktop_keys "$f"
done

echo
if [ "$FAIL" -ne 0 ]; then
  echo "session packaging verification FAILED" >&2
  exit 1
fi
echo "session packaging verification PASSED"
exit 0

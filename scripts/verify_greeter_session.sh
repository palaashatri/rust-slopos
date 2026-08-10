#!/usr/bin/env bash
# verify_greeter_session.sh — packaging + session-script smoke for greeter installs.
#
# Does NOT require a live display manager. Checks that:
#   1) packaging desktop files validate (via verify_session_packaging.sh)
#   2) start-slopos-i is executable and --help or dry-run path is sane
#   3) Required env defaults for XDG session are documentable
#
# Exit 0 on success. Used in CI / Docker image smoke.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

echo "==> packaging tree"
bash "$ROOT/scripts/verify_session_packaging.sh"

echo "==> start-slopos-i is executable"
test -x "$ROOT/scripts/start-slopos-i"

echo "==> start-slopos-i documents compositor selection"
grep -q "SLOPOS_COMPOSITOR\|labwc\|slopos-compositor" "$ROOT/scripts/start-slopos-i"

echo "==> desktop files point at start-slopos-i"
grep -q "Exec=start-slopos-i" "$ROOT/packaging/slopos-i.desktop"
grep -q "Exec=start-slopos-i" "$ROOT/packaging/slopos-i-wayland.desktop"

echo "==> wayland session Type/DesktopNames"
grep -q "Type=Application" "$ROOT/packaging/slopos-i-wayland.desktop" \
  || grep -q "Type=Application" "$ROOT/packaging/slopos-i.desktop"

echo "==> systemd user unit ExecStart"
grep -q "start-slopos-i" "$ROOT/packaging/slopos-i.service"

echo "==> DesktopNames=SLOPOS-I on both session desktops"
grep -q "DesktopNames=SLOPOS-I" "$ROOT/packaging/slopos-i.desktop"
grep -q "DesktopNames=SLOPOS-I" "$ROOT/packaging/slopos-i-wayland.desktop"

echo "==> TryExec present on both session desktops (greeter can probe binary)"
grep -q "TryExec=start-slopos-i" "$ROOT/packaging/slopos-i.desktop"
grep -q "TryExec=start-slopos-i" "$ROOT/packaging/slopos-i-wayland.desktop"

echo "==> Keywords consistent on both session desktops"
grep -q "Keywords=SLOPOS-I;Wayland;Desktop;" "$ROOT/packaging/slopos-i.desktop"
grep -q "Keywords=SLOPOS-I;Wayland;Desktop;" "$ROOT/packaging/slopos-i-wayland.desktop"

echo "==> install-session-files.sh dry-run"
test -x "$ROOT/scripts/install-session-files.sh"
DRY_LOG="$(mktemp)"
"$ROOT/scripts/install-session-files.sh" --dry-run --prefix /tmp/slopos-i-greeter-dryrun >"$DRY_LOG"
grep -q "wayland-sessions/slopos-i.desktop" "$DRY_LOG"
grep -q "bin/start-slopos-i" "$DRY_LOG"
rm -f "$DRY_LOG"

echo "==> session_entry_smoke_report source is honest (no live greeter claim)"
# Structural evidence only — never assert live DM. The Rust report hard-codes
# live_greeter_verified: false; this script only checks packaging + that honesty.
grep -q "session_entry_smoke_report" "$ROOT/crates/slopos-shell/src/session_packaging.rs"
grep -q "live_greeter_verified: false" "$ROOT/crates/slopos-shell/src/session_packaging.rs"
grep -q "live_greeter_verified" "$ROOT/crates/slopos-shell/src/session_packaging.rs"

echo
echo "greeter session packaging smoke PASSED (no live DM required)"
echo "NOTE: live_greeter_verified remains false — packaging evidence only"
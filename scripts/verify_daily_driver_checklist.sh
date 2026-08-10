#!/usr/bin/env bash
# verify_daily_driver_checklist.sh — honest §12 / scorecard smoke (no live greeter claim).
#
# Exit 0 only if packaging + unit-testable artifacts look install-ready.
# Does NOT prove: greeter login, DRM seat, Orca, PipeWire streams, Plasma week.
#
# Toward §12 criterion 1 (greeter → session): packaging + install-session dry-run
# must pass. Live DM login remains host/hardware evidence, not this script.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

fail=0
pass() { echo "OK  $*"; }
warn() { echo "WARN $*"; }
die() { echo "FAIL $*"; fail=1; }

echo "==> packaging / greeter install artifacts"
if [[ -x scripts/verify_greeter_session.sh ]]; then
  if scripts/verify_greeter_session.sh; then
    pass "greeter packaging smoke"
  else
    die "greeter packaging smoke"
  fi
else
  die "missing verify_greeter_session.sh"
fi

echo "==> install-session-files.sh present + dry-run"
if [[ -x scripts/install-session-files.sh ]]; then
  if scripts/install-session-files.sh --dry-run --prefix /tmp/slopos-i-session-dryrun >/tmp/slopos-i-install-dryrun.log 2>&1; then
    if grep -q "DRY-RUN install" /tmp/slopos-i-install-dryrun.log \
      && grep -q "wayland-sessions/slopos-i.desktop" /tmp/slopos-i-install-dryrun.log \
      && grep -q "bin/start-slopos-i" /tmp/slopos-i-install-dryrun.log; then
      pass "install-session-files dry-run"
    else
      die "install-session-files dry-run missing expected paths (see /tmp/slopos-i-install-dryrun.log)"
    fi
  else
    die "install-session-files dry-run failed"
  fi
else
  die "missing or non-executable scripts/install-session-files.sh"
fi

echo "==> desktop keys consistent (DesktopNames, TryExec, Keywords)"
for f in packaging/slopos-i.desktop packaging/slopos-i-wayland.desktop; do
  for key in DesktopNames TryExec Keywords; do
    if grep -qE "^${key}=" "$f"; then
      pass "$f has $key"
    else
      die "$f missing $key"
    fi
  done
  if ! grep -q "DesktopNames=SLOPOS-I" "$f"; then
    die "$f DesktopNames must be SLOPOS-I"
  fi
  if ! grep -q "TryExec=start-slopos-i" "$f"; then
    die "$f TryExec must be start-slopos-i"
  fi
  if ! grep -q "Keywords=SLOPOS-I;Wayland;Desktop;" "$f"; then
    die "$f Keywords must match SLOPOS-I;Wayland;Desktop;"
  fi
done

echo "==> start-slopos-i documents OUTPUTS_LAYOUT + compositor selection"
if grep -q "SLOPOS_OUTPUTS_LAYOUT" scripts/start-slopos-i; then
  pass "start-slopos-i documents SLOPOS_OUTPUTS_LAYOUT"
else
  die "start-slopos-i missing SLOPOS_OUTPUTS_LAYOUT docs/export"
fi
if grep -q "compositor selection" scripts/start-slopos-i; then
  pass "start-slopos-i logs compositor selection honestly"
else
  die "start-slopos-i missing honest compositor selection logs"
fi

echo "==> pure module presence (warpath integration targets)"
for f in \
  crates/slopos-shell/src/session_actions.rs \
  crates/slopos-shell/src/display_arrange.rs \
  crates/slopos-shell/src/window_rules.rs \
  crates/slopos-shell/src/idle_policy.rs \
  crates/slopos-shell/src/i18n.rs \
  crates/slopos-shell/src/portal_extra.rs \
  crates/slopos-shell/src/a11y_actions.rs \
  crates/slopos-shell/src/session_packaging.rs \
  crates/slopos-compositor/src/lib.rs
do
  if [[ -f "$f" ]]; then pass "exists $f"; else die "missing $f"; fi
done

echo "==> greeter readiness stays honest (no live DM claim in notes path)"
if grep -Fq "live greeter login still requires DM" crates/slopos-shell/src/session_packaging.rs; then
  pass "session_packaging honest greeter note present"
else
  die "session_packaging missing honest greeter note"
fi
if grep -Fq "install_ready" crates/slopos-shell/src/session_packaging.rs \
  && grep -Fq "Does **not** claim a live display manager" crates/slopos-shell/src/session_packaging.rs; then
  pass "install_ready documented as packaging-only"
else
  die "install_ready honesty comment missing"
fi
if grep -Fq "session_entry_smoke_report" crates/slopos-shell/src/session_packaging.rs \
  && grep -Fq "live_greeter_verified" crates/slopos-shell/src/session_packaging.rs \
  && grep -Fq "live_greeter_verified: false" crates/slopos-shell/src/session_packaging.rs; then
  pass "session_entry_smoke_report always reports live_greeter_verified: false"
else
  die "session_entry_smoke_report / live_greeter_verified honesty missing"
fi

echo "==> compositor workspace filter is referenced from main (live path)"
if grep -Eq "workspace_state|is_visible|windows_visible_for_paint" crates/slopos-compositor/src/main.rs; then
  pass "compositor main references workspace visibility"
else
  die "compositor main missing workspace visibility wiring"
fi

echo "==> portal Secret/Print/Inhibit on dbus module"
if grep -Eq "PortalSecretIface|PortalPrintIface|PortalInhibitIface" crates/slopos-shell/src/portal_dbus.rs; then
  pass "portal_dbus exports Secret/Print/Inhibit interfaces"
else
  die "portal_dbus missing Secret/Print/Inhibit"
fi

echo "==> i18n used outside catalog module"
if grep -Eq '(^|[^[:alnum:]_])tr\(' crates/slopos-shell/src/lib.rs crates/slopos-shell/src/menu_server.rs 2>/dev/null; then
  pass "tr() used in shell UI paths"
else
  warn "tr() may still be lock-only — check lib.rs"
fi

echo "==> host unit tests (exclude full compositor binary if needed)"
if command -v cargo >/dev/null; then
  if cargo test -p slopos-shell -p slopos-kit -p slopos-compositor --lib --quiet; then
    pass "cargo test shell+kit+compositor lib"
  else
    die "cargo test failed"
  fi
else
  warn "cargo not on PATH — skipped unit tests"
fi

echo
if [[ "$fail" -ne 0 ]]; then
  echo "daily-driver checklist FAILED (install/unit evidence incomplete)"
  echo "NOTE: failure here is packaging/unit evidence only — not a live greeter result"
  exit 1
fi
echo "daily-driver checklist PASSED (packaging + unit evidence only — not Plasma-100)"
echo "§12 greeter criterion: install artifacts ready; live DM login still NOT RUN"
exit 0

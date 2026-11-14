#!/usr/bin/env bash
# run-calculator-qa.sh: keyboard, pointer, accessibility and visual QA for
# Calculator on a private Xvfb display with Openbox and an AT-SPI bus.
#
# Usage:
#   scripts/run-calculator-qa.sh --bin-dir DIR [OUTPUT_DIR]
#       Test a build: DIR holds slopos-calculator; the desktop entry comes
#       from packaging/.
#   scripts/run-calculator-qa.sh --deb FILE [OUTPUT_DIR]
#       Test a Debian package: extract it into a throwaway root and launch
#       Calculator from the packaged desktop entry and binary.
#
#   CALC_QA_DISPLAY  X display to use (default :93)
#   CALC_QA_SCALES   UI scales to test (default "1 2")
#   PYTHON           Python with the AT-SPI GObject bindings (found if unset)
#   CALC_QA_BASELINE_BIN  an earlier slopos-calculator to compare renders with
#
# Needs: Xvfb, openbox, dbus-run-session, at-spi-bus-launcher, gdbus,
# xdotool, ImageMagick (import, compare, identify) and python3-gi with Atspi.
# Exits non-zero if any check fails; CALCULATOR_QA_STATUS_0 means all passed.
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
mode="${1:-}"
source_path="${2:-}"
[[ "$mode" == --bin-dir || "$mode" == --deb ]] && [[ -n "$source_path" ]] || {
  sed -n '2,20p' "$0" >&2
  exit 2
}
OUT="$(mkdir -p "${3:-$ROOT/artifacts/calculator-qa}" && cd "${3:-$ROOT/artifacts/calculator-qa}" && pwd)"
export DISPLAY="${CALC_QA_DISPLAY:-:93}"
SCALES="${CALC_QA_SCALES:-1 2}"

for tool in Xvfb openbox dbus-run-session gdbus xdotool import compare identify; do
  command -v "$tool" >/dev/null || { echo "calculator-qa: $tool is required" >&2; exit 2; }
done
LAUNCHER="$(command -v at-spi-bus-launcher || echo /usr/libexec/at-spi-bus-launcher)"
[[ -x "$LAUNCHER" ]] || { echo "calculator-qa: at-spi-bus-launcher is required" >&2; exit 2; }

# Distribution python3 and the python3-gi extension can disagree on version;
# use whichever interpreter actually loads Atspi.
if [[ -z "${PYTHON:-}" ]]; then
  for candidate in python3 python3.13 python3.12 python3.11; do
    if command -v "$candidate" >/dev/null &&
      "$candidate" -c 'import gi; gi.require_version("Atspi", "2.0"); from gi.repository import Atspi' 2>/dev/null; then
      PYTHON="$candidate"
      break
    fi
  done
fi
[[ -n "${PYTHON:-}" ]] || { echo "calculator-qa: no Python with gi.repository.Atspi" >&2; exit 2; }

WORK="$(mktemp -d)"
XVFB_PID=""
cleanup() {
  set +e
  [[ -n "$XVFB_PID" ]] && kill "$XVFB_PID" 2>/dev/null
  rm -rf "$WORK"
}
trap cleanup EXIT

if [[ "$mode" == --deb ]]; then
  deb="$(cd "$(dirname "$source_path")" && pwd)/$(basename "$source_path")"
  dpkg-deb --extract "$deb" "$WORK/root"
  BIN_DIR="$WORK/root/usr/bin"
  DESKTOP_FILE="$WORK/root/usr/share/applications/slopos-calculator.desktop"
  OPENBOX_RC="$WORK/root/usr/share/slopos-i/openbox/rc.xml"
  sha256sum "$deb" >"$OUT/package.sha256"
else
  BIN_DIR="$(cd "$source_path" && pwd)"
  DESKTOP_FILE="$ROOT/packaging/slopos-calculator.desktop"
  OPENBOX_RC="$ROOT/assets/config/openbox/rc.xml"
fi
for path in "$BIN_DIR/slopos-calculator" "$DESKTOP_FILE" "$OPENBOX_RC"; do
  [[ -e "$path" ]] || { echo "calculator-qa: missing $path" >&2; exit 2; }
done

if xdotool getdisplaygeometry >/dev/null 2>&1; then
  echo "calculator-qa: display $DISPLAY is in use; set CALC_QA_DISPLAY" >&2
  exit 2
fi
Xvfb "$DISPLAY" -screen 0 1280x800x24 -nolisten tcp >"$OUT/xvfb.log" 2>&1 &
XVFB_PID=$!
for _ in $(seq 1 50); do xdotool getdisplaygeometry >/dev/null 2>&1 && break; sleep 0.1; done
xdotool getdisplaygeometry >/dev/null 2>&1 || { echo "calculator-qa: Xvfb did not start" >&2; exit 1; }

export HOME="$WORK/home" XDG_RUNTIME_DIR="$WORK/run" XDG_CONFIG_HOME="$WORK/home/.config"
mkdir -p "$HOME" "$XDG_RUNTIME_DIR"
chmod 700 "$XDG_RUNTIME_DIR"
export LAUNCHER OPENBOX_RC BIN_DIR DESKTOP_FILE OUT SCALES PYTHON ROOT

status=0
dbus-run-session -- bash -c '
  set -euo pipefail
  "$LAUNCHER" --launch-immediately >"$OUT/atspi-bus.log" 2>&1 &
  for _ in $(seq 1 50); do
    gdbus call --session --dest org.a11y.Bus --object-path /org/a11y/bus \
      --method org.a11y.Bus.GetAddress >/dev/null 2>&1 && break
    sleep 0.1
  done
  # Toolkits only export their trees once assistive technology is enabled.
  gdbus call --session --dest org.a11y.Bus --object-path /org/a11y/bus \
    --method org.freedesktop.DBus.Properties.Set org.a11y.Status IsEnabled "<true>" >/dev/null
  openbox --config-file "$OPENBOX_RC" >"$OUT/openbox.log" 2>&1 &
  sleep 1
  failed=0
  for scale in $SCALES; do
    echo "=== Calculator at ${scale}x ==="
    "$PYTHON" -I "$ROOT/scripts/qa-calculator.py" --bin-dir "$BIN_DIR" \
      --desktop-file "$DESKTOP_FILE" --scale "$scale" --out "$OUT" || failed=1
  done
  exit "$failed"
' 2>&1 | tee "$OUT/calculator-qa.log" || status=1

# Optional cosmetic comparison with an earlier build, such as the last nightly.
# Any difference is written as a diff image for a person to review; it is not
# a failure by itself, because a change may be intended.
if [[ -n "${CALC_QA_BASELINE_BIN:-}" ]]; then
  for scale in $SCALES; do
    SLOPOS_SCALE="$scale" "$CALC_QA_BASELINE_BIN" --snapshot "$OUT/baseline-${scale}x.png"
    SLOPOS_SCALE="$scale" "$BIN_DIR/slopos-calculator" --snapshot "$OUT/current-${scale}x.png"
    differing="$(compare -metric AE "$OUT/baseline-${scale}x.png" "$OUT/current-${scale}x.png" \
      "$OUT/baseline-diff-${scale}x.png" 2>&1 >/dev/null || true)"
    if [[ "$differing" == 0 ]]; then
      rm -f "$OUT/baseline-diff-${scale}x.png"
      echo "BASELINE ${scale}x: identical to $CALC_QA_BASELINE_BIN" | tee -a "$OUT/calculator-qa.log"
    else
      echo "BASELINE ${scale}x: REVIEW $differing pixels differ; see baseline-diff-${scale}x.png" |
        tee -a "$OUT/calculator-qa.log"
    fi
  done
fi

if [[ "$status" == 0 ]] && ! grep -q '^FAIL ' "$OUT/calculator-qa.log"; then
  echo "CALCULATOR_QA_STATUS_0"
else
  echo "CALCULATOR_QA_STATUS_1: see $OUT/calculator-qa.log" >&2
  exit 1
fi

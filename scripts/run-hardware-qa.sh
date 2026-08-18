#!/usr/bin/env bash
# Physical-machine evidence collector for SLOPOS-I.
#
# Safe probes are the default. Mutating audio/Bluetooth state and a real
# suspend/resume cycle are explicit opt-ins because this script is intended to
# be run by a human on representative hardware, never silently from hosted CI.
set -euo pipefail

OUTPUT_DIR="${SLOPOS_HARDWARE_QA_OUTPUT_DIR:-$PWD/artifacts/hardware-qa}"
EXPECTED_COMMIT="${SLOPOS_HARDWARE_QA_EXPECTED_COMMIT:-}"
SOURCE_ROOT="${SLOPOS_HARDWARE_QA_SOURCE_ROOT:-$PWD}"
MIN_REFRESH_HZ="${SLOPOS_HARDWARE_QA_MIN_REFRESH_HZ:-}"
REQUIRE_WIFI="${SLOPOS_HARDWARE_QA_REQUIRE_WIFI:-0}"
REQUIRE_BLUETOOTH="${SLOPOS_HARDWARE_QA_REQUIRE_BLUETOOTH:-0}"
REQUIRE_BATTERY="${SLOPOS_HARDWARE_QA_REQUIRE_BATTERY:-0}"
REQUIRE_GL="${SLOPOS_HARDWARE_QA_REQUIRE_GL:-1}"
MUTATE_AUDIO="${SLOPOS_HARDWARE_QA_MUTATE_AUDIO:-0}"
MUTATE_BLUETOOTH="${SLOPOS_HARDWARE_QA_MUTATE_BLUETOOTH:-0}"
SUSPEND_RESUME="${SLOPOS_HARDWARE_QA_SUSPEND_RESUME:-0}"
SUSPEND_SETTLE_SECONDS="${SLOPOS_HARDWARE_QA_SUSPEND_SETTLE_SECONDS:-8}"
DISPLAY="${DISPLAY:-:0}"
export DISPLAY

fail() { echo "HARDWARE_QA_ERROR: $*" >&2; exit 1; }
warn() { echo "HARDWARE_QA_WARNING: $*" >&2; }
need() { command -v "$1" >/dev/null 2>&1 || fail "missing required command: $1"; }

for value_name in REQUIRE_WIFI REQUIRE_BLUETOOTH REQUIRE_BATTERY REQUIRE_GL MUTATE_AUDIO MUTATE_BLUETOOTH SUSPEND_RESUME; do
  value="${!value_name}"
  [[ "$value" == 0 || "$value" == 1 ]] || fail "$value_name must be 0 or 1"
done
[[ "$SUSPEND_SETTLE_SECONDS" =~ ^[0-9]+$ ]] || fail "SLOPOS_HARDWARE_QA_SUSPEND_SETTLE_SECONDS must be an integer"
case "$OUTPUT_DIR" in /*) ;; *) OUTPUT_DIR="$PWD/$OUTPUT_DIR" ;; esac
case "$SOURCE_ROOT" in /*) ;; *) SOURCE_ROOT="$PWD/$SOURCE_ROOT" ;; esac
mkdir -p "$OUTPUT_DIR"
OUTPUT_DIR="$(cd "$OUTPUT_DIR" && pwd)"
SOURCE_ROOT="$(cd "$SOURCE_ROOT" && pwd)"

for command in git xrandr xdpyinfo pgrep ps awk sed grep date sha256sum; do need "$command"; done

LOG="$OUTPUT_DIR/hardware-qa.log"
MANIFEST="$OUTPUT_DIR/evidence-manifest.txt"
STATUS="$OUTPUT_DIR/status.env"
: > "$LOG"
: > "$MANIFEST"
exec > >(tee -a "$LOG") 2>&1

# Mutating probes must restore host state even if a later assertion fails.
audio_restore_pending=0
default_sink=""
original_mute=""
bluetooth_restore_pending=0
bluetooth_controller=""
original_power=""
cleanup() {
  status=$?
  set +e
  if [[ "$audio_restore_pending" == 1 && -n "$default_sink" && -n "$original_mute" ]] && command -v pactl >/dev/null 2>&1; then
    pactl set-sink-mute "$default_sink" "$original_mute" >/dev/null 2>&1 || true
  fi
  if [[ "$bluetooth_restore_pending" == 1 && -n "$bluetooth_controller" && -n "$original_power" ]] && command -v bluetoothctl >/dev/null 2>&1; then
    if [[ "$original_power" == yes ]]; then
      bluetoothctl power on >/dev/null 2>&1 || true
    else
      bluetoothctl power off >/dev/null 2>&1 || true
    fi
  fi
  exit "$status"
}
trap cleanup EXIT

started_utc="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo "SLOPOS_HARDWARE_QA_STARTED_UTC=$started_utc"
echo "display=$DISPLAY" >> "$MANIFEST"

step() { printf '\n=== %s ===\n' "$*"; }

step "bind evidence to exact source"
test -e "$SOURCE_ROOT/.git" || fail "source checkout is missing at $SOURCE_ROOT"
source_commit="$(git -C "$SOURCE_ROOT" rev-parse --verify HEAD)"
[[ "$source_commit" =~ ^[0-9a-fA-F]{40}$ ]] || fail "source checkout did not report a full commit SHA"
if [[ -n "$EXPECTED_COMMIT" ]]; then
  [[ "$EXPECTED_COMMIT" =~ ^[0-9a-fA-F]{40}$ ]] || fail "expected commit must be a full SHA"
  [[ "${source_commit,,}" == "${EXPECTED_COMMIT,,}" ]] || fail "source commit $source_commit does not match expected $EXPECTED_COMMIT"
fi
printf 'source_commit=%s\n' "$source_commit" >> "$MANIFEST"
printf 'kernel=%s\n' "$(uname -srmo)" >> "$MANIFEST"
printf 'hostname=%s\n' "$(hostname)" >> "$MANIFEST"
if [[ -r /etc/os-release ]]; then
  cp /etc/os-release "$OUTPUT_DIR/os-release"
fi
echo "HARDWARE_SOURCE_COMMIT_STATUS_0=$source_commit"

step "verify real SLOPOS X11 session"
xdpyinfo -display "$DISPLAY" > "$OUTPUT_DIR/xdpyinfo.txt"
pgrep -x openbox >/dev/null || fail "Openbox is not running"
pgrep -x slopos-shell >/dev/null || fail "slopos-shell is not running"
test "$(pgrep -xc slopos-shell)" -eq 1 || fail "expected exactly one slopos-shell"
pgrep -x slopos-session >/dev/null || fail "slopos-session is not running"
test "$(pgrep -xc slopos-session)" -eq 1 || fail "expected exactly one slopos-session"
ps -eo pid,ppid,comm,args > "$OUTPUT_DIR/processes.txt"
echo "HARDWARE_X11_SESSION_STATUS_0"

step "physical display and refresh evidence"
xrandr --current > "$OUTPUT_DIR/xrandr-current.txt"
xrandr --verbose > "$OUTPUT_DIR/xrandr-verbose.txt"
xrandr --listproviders > "$OUTPUT_DIR/xrandr-providers.txt" 2>&1 || true
connected_count="$(awk '$2 == "connected" {count++} END {print count+0}' "$OUTPUT_DIR/xrandr-current.txt")"
(( connected_count > 0 )) || fail "XRandR reports no connected display"
active_mode_count="$(awk '
  /^[[:space:]]+[0-9]+x[0-9]+/ {
    for (i=2; i<=NF; i++) if ($i ~ /\*/) {count++; break}
  }
  END {print count+0}
' "$OUTPUT_DIR/xrandr-current.txt")"
(( active_mode_count > 0 )) || fail "XRandR reports no active display mode"
max_active_refresh="$(awk '
  /^[[:space:]]+[0-9]+x[0-9]+/ {
    for (i=2; i<=NF; i++) {
      if ($i !~ /\*/) continue
      rate=$i
      gsub(/[+*]/, "", rate)
      if (rate ~ /^[0-9]+([.][0-9]+)?$/ && rate+0 > max) max=rate+0
    }
  }
  END { if (max > 0) printf "%.3f", max }
' "$OUTPUT_DIR/xrandr-current.txt")"
test -n "$max_active_refresh" || fail "could not determine active refresh rate"
printf 'connected_displays=%s\nactive_modes=%s\nmax_active_refresh_hz=%s\n' \
  "$connected_count" "$active_mode_count" "$max_active_refresh" >> "$MANIFEST"
if [[ -n "$MIN_REFRESH_HZ" ]]; then
  [[ "$MIN_REFRESH_HZ" =~ ^[0-9]+([.][0-9]+)?$ ]] || fail "minimum refresh must be numeric"
  awk -v actual="$max_active_refresh" -v minimum="$MIN_REFRESH_HZ" 'BEGIN { exit !(actual+0 >= minimum+0) }' ||
    fail "active refresh ${max_active_refresh}Hz is below requested ${MIN_REFRESH_HZ}Hz"
  echo "HARDWARE_REFRESH_THRESHOLD_STATUS_0=$MIN_REFRESH_HZ"
fi
if command -v xprop >/dev/null 2>&1; then
  xprop -root > "$OUTPUT_DIR/x11-root-properties.txt"
fi
echo "HARDWARE_DISPLAY_STATUS_0=outputs:$connected_count active_modes:$active_mode_count refresh_hz:$max_active_refresh"

step "GPU and DRM evidence"
if command -v lspci >/dev/null 2>&1; then
  lspci -nnk > "$OUTPUT_DIR/lspci-nnk.txt"
  lspci -nnk | grep -A4 -Ei 'VGA compatible controller|3D controller|Display controller' > "$OUTPUT_DIR/gpu-pci.txt" || true
fi
{
  for card in /sys/class/drm/card[0-9]*; do
    [[ -e "$card" ]] || continue
    echo "card=$(basename "$card")"
    driver="$(readlink -f "$card/device/driver" 2>/dev/null || true)"
    [[ -n "$driver" ]] && echo "driver=$driver"
    [[ -r "$card/device/vendor" ]] && echo "vendor=$(< "$card/device/vendor")"
    [[ -r "$card/device/device" ]] && echo "device=$(< "$card/device/device")"
  done
} > "$OUTPUT_DIR/drm-devices.txt"
test -s "$OUTPUT_DIR/drm-devices.txt" || warn "no DRM card metadata was readable"
if command -v glxinfo >/dev/null 2>&1; then
  glxinfo -B > "$OUTPUT_DIR/glxinfo.txt"
  renderer="$(sed -nE 's/^[[:space:]]*OpenGL renderer string:[[:space:]]*//p' "$OUTPUT_DIR/glxinfo.txt" | head -1)"
  test -n "$renderer" || fail "GLX renderer identity is unavailable"
  printf 'gl_renderer=%s\n' "$renderer" >> "$MANIFEST"
  if grep -Eiq 'llvmpipe|softpipe|software rasterizer' <<<"$renderer"; then
    if [[ "$REQUIRE_GL" == 1 ]]; then
      fail "software GL renderer detected on physical-hardware QA: $renderer"
    fi
    warn "software GL renderer detected: $renderer"
  fi
  echo "HARDWARE_GLX_STATUS_0=$renderer"
elif [[ "$REQUIRE_GL" == 1 ]]; then
  fail "glxinfo is required for physical GPU renderer evidence"
else
  warn "glxinfo is unavailable; GL renderer identity is not captured"
fi
echo "HARDWARE_GPU_PROBE_STATUS_0"

step "NetworkManager and Wi-Fi evidence"
if command -v systemctl >/dev/null 2>&1 && systemctl is-active --quiet NetworkManager; then
  echo "networkmanager=active" >> "$MANIFEST"
else
  fail "NetworkManager is not active"
fi
need nmcli
nmcli general status > "$OUTPUT_DIR/nm-general.txt"
nmcli -f GENERAL.DEVICE,GENERAL.TYPE,GENERAL.STATE,GENERAL.CONNECTION device show > "$OUTPUT_DIR/nm-devices.txt"
nmcli -t -f DEVICE,TYPE,STATE,CONNECTION device status > "$OUTPUT_DIR/nm-device-status.txt"
wifi_device="$(nmcli -t -f DEVICE,TYPE device status | awk -F: '$2 == "wifi" {print $1; exit}')"
if [[ -n "$wifi_device" ]]; then
  echo "wifi_device=$wifi_device" >> "$MANIFEST"
  nmcli radio wifi > "$OUTPUT_DIR/wifi-radio.txt"
  # A rescan is bounded and does not intentionally disconnect an active
  # connection. Its success proves NetworkManager can command the adapter.
  if nmcli device wifi rescan ifname "$wifi_device"; then
    echo "HARDWARE_WIFI_RESCAN_STATUS_0=$wifi_device"
  elif [[ "$REQUIRE_WIFI" == 1 ]]; then
    fail "Wi-Fi adapter exists but a hardware rescan failed"
  else
    warn "Wi-Fi adapter exists but rescan failed"
  fi
elif [[ "$REQUIRE_WIFI" == 1 ]]; then
  fail "Wi-Fi hardware was required but NetworkManager exposes no Wi-Fi device"
else
  warn "no Wi-Fi device exposed by NetworkManager"
fi
echo "HARDWARE_NETWORK_STATUS_0"

step "PipeWire/WirePlumber audio evidence"
need pactl
pactl info > "$OUTPUT_DIR/pactl-info.txt"
pactl list short sinks > "$OUTPUT_DIR/pactl-sinks.txt"
pactl list short sources > "$OUTPUT_DIR/pactl-sources.txt"
default_sink="$(pactl get-default-sink 2>/dev/null || true)"
test -n "$default_sink" || fail "PipeWire/Pulse compatibility layer reports no default sink"
printf 'default_audio_sink=%s\n' "$default_sink" >> "$MANIFEST"
if [[ "$MUTATE_AUDIO" == 1 ]]; then
  original_mute="$(pactl get-sink-mute "$default_sink" | awk '{print $2}')"
  [[ "$original_mute" == yes || "$original_mute" == no ]] || fail "cannot read default-sink mute state"
  audio_restore_pending=1
  if [[ "$original_mute" == yes ]]; then target_mute=no; else target_mute=yes; fi
  pactl set-sink-mute "$default_sink" "$target_mute"
  observed_mute="$(pactl get-sink-mute "$default_sink" | awk '{print $2}')"
  [[ "$observed_mute" == "$target_mute" ]] || fail "audio mute mutation did not apply"
  pactl set-sink-mute "$default_sink" "$original_mute"
  observed_restore="$(pactl get-sink-mute "$default_sink" | awk '{print $2}')"
  [[ "$observed_restore" == "$original_mute" ]] || fail "audio mute state did not restore"
  audio_restore_pending=0
  echo "HARDWARE_AUDIO_MUTATION_STATUS_0=$default_sink"
fi
echo "HARDWARE_AUDIO_STATUS_0=$default_sink"

step "BlueZ controller evidence"
bluetooth_controller=""
if command -v bluetoothctl >/dev/null 2>&1; then
  bluetoothctl list > "$OUTPUT_DIR/bluetooth-list.txt" || true
  bluetooth_controller="$(awk '/^Controller / {print $2; exit}' "$OUTPUT_DIR/bluetooth-list.txt")"
fi
if [[ -n "$bluetooth_controller" ]]; then
  bluetoothctl show "$bluetooth_controller" > "$OUTPUT_DIR/bluetooth-show.txt"
  printf 'bluetooth_controller=%s\n' "$bluetooth_controller" >> "$MANIFEST"
  if [[ "$MUTATE_BLUETOOTH" == 1 ]]; then
    original_power="$(awk -F': ' '$1 ~ /Powered$/ {print $2; exit}' "$OUTPUT_DIR/bluetooth-show.txt")"
    [[ "$original_power" == yes || "$original_power" == no ]] || fail "cannot read Bluetooth controller power state"
    bluetooth_restore_pending=1
    if [[ "$original_power" == yes ]]; then target_power=off; else target_power=on; fi
    bluetoothctl power "$target_power" >/dev/null
    sleep 1
    bluetoothctl show "$bluetooth_controller" > "$OUTPUT_DIR/bluetooth-mutated.txt"
    observed_power="$(awk -F': ' '$1 ~ /Powered$/ {print $2; exit}' "$OUTPUT_DIR/bluetooth-mutated.txt")"
    if [[ "$target_power" == on ]]; then expected_power=yes; else expected_power=no; fi
    [[ "$observed_power" == "$expected_power" ]] || fail "Bluetooth power mutation did not apply"
    if [[ "$original_power" == yes ]]; then bluetoothctl power on >/dev/null; else bluetoothctl power off >/dev/null; fi
    sleep 1
    bluetoothctl show "$bluetooth_controller" > "$OUTPUT_DIR/bluetooth-restored.txt"
    restored_power="$(awk -F': ' '$1 ~ /Powered$/ {print $2; exit}' "$OUTPUT_DIR/bluetooth-restored.txt")"
    [[ "$restored_power" == "$original_power" ]] || fail "Bluetooth power state did not restore"
    bluetooth_restore_pending=0
    echo "HARDWARE_BLUETOOTH_MUTATION_STATUS_0=$bluetooth_controller"
  fi
  echo "HARDWARE_BLUETOOTH_STATUS_0=$bluetooth_controller"
elif [[ "$REQUIRE_BLUETOOTH" == 1 ]]; then
  fail "Bluetooth hardware was required but BlueZ exposes no controller"
else
  warn "BlueZ exposes no Bluetooth controller"
fi

step "UPower and battery evidence"
if command -v upower >/dev/null 2>&1; then
  upower -e > "$OUTPUT_DIR/upower-devices.txt"
  battery="$(grep -E '/battery(_|$)|/BAT[0-9]+$' "$OUTPUT_DIR/upower-devices.txt" | head -1 || true)"
  if [[ -n "$battery" ]]; then
    upower -i "$battery" > "$OUTPUT_DIR/upower-battery.txt"
    grep -Eq 'state:|percentage:' "$OUTPUT_DIR/upower-battery.txt" || fail "battery device lacks state/percentage"
    printf 'battery_device=%s\n' "$battery" >> "$MANIFEST"
    echo "HARDWARE_BATTERY_STATUS_0=$battery"
  elif [[ "$REQUIRE_BATTERY" == 1 ]]; then
    fail "battery hardware was required but UPower exposes no battery"
  else
    warn "UPower exposes no battery (expected on desktops)"
  fi
else
  [[ "$REQUIRE_BATTERY" == 0 ]] || fail "upower is required for battery evidence"
  warn "upower is unavailable"
fi

if [[ "$SUSPEND_RESUME" == 1 ]]; then
  step "opt-in physical suspend/resume"
  need systemctl
  need loginctl
  if [[ -r /sys/power/state ]]; then
    grep -qw mem /sys/power/state || fail "kernel does not advertise suspend-to-RAM"
  fi
  pre_suspend_boot_id="$(cat /proc/sys/kernel/random/boot_id)"
  pre_suspend_uptime="$(awk '{print $1}' /proc/uptime)"
  pre_shell_pid="$(pgrep -xo slopos-shell)"
  pre_openbox_pid="$(pgrep -xo openbox)"
  printf 'pre_suspend_utc=%s\npre_suspend_boot_id=%s\npre_suspend_uptime=%s\n' \
    "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$pre_suspend_boot_id" "$pre_suspend_uptime" >> "$MANIFEST"
  sync
  echo "About to suspend this physical machine; QA resumes after wake."
  systemctl suspend
  sleep "$SUSPEND_SETTLE_SECONDS"
  post_suspend_boot_id="$(cat /proc/sys/kernel/random/boot_id)"
  [[ "$post_suspend_boot_id" == "$pre_suspend_boot_id" ]] || fail "machine rebooted instead of resuming"
  xdpyinfo -display "$DISPLAY" >/dev/null || fail "X11 display is unavailable after resume"
  pgrep -x slopos-shell >/dev/null || fail "slopos-shell is unavailable after resume"
  pgrep -x openbox >/dev/null || fail "Openbox is unavailable after resume"
  xrandr --current > "$OUTPUT_DIR/xrandr-after-resume.txt"
  nmcli general status > "$OUTPUT_DIR/nm-after-resume.txt"
  pactl info > "$OUTPUT_DIR/pactl-after-resume.txt"
  printf 'post_resume_utc=%s\npost_resume_uptime=%s\npre_shell_pid=%s\npost_shell_pid=%s\npre_openbox_pid=%s\npost_openbox_pid=%s\n' \
    "$(date -u +%Y-%m-%dT%H:%M:%SZ)" "$(awk '{print $1}' /proc/uptime)" \
    "$pre_shell_pid" "$(pgrep -xo slopos-shell)" "$pre_openbox_pid" "$(pgrep -xo openbox)" >> "$MANIFEST"
  echo "HARDWARE_SUSPEND_RESUME_STATUS_0"
fi

step "finalize evidence bundle"
completed_utc="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
{
  echo "SLOPOS_HARDWARE_QA_STATUS_0"
  echo "source_commit=$source_commit"
  echo "started_utc=$started_utc"
  echo "completed_utc=$completed_utc"
  echo "connected_displays=$connected_count"
  echo "active_modes=$active_mode_count"
  echo "max_active_refresh_hz=$max_active_refresh"
  echo "wifi_device=${wifi_device:-none}"
  echo "bluetooth_controller=${bluetooth_controller:-none}"
  echo "default_audio_sink=$default_sink"
  echo "audio_mutation=$MUTATE_AUDIO"
  echo "bluetooth_mutation=$MUTATE_BLUETOOTH"
  echo "suspend_resume=$SUSPEND_RESUME"
} > "$STATUS"

find "$OUTPUT_DIR" -maxdepth 1 -type f ! -name 'SHA256SUMS' -print0 \
  | sort -z \
  | xargs -0 sha256sum > "$OUTPUT_DIR/SHA256SUMS"
echo "SLOPOS_HARDWARE_QA_STATUS_0"
echo "Evidence directory: $OUTPUT_DIR"
trap - EXIT
cleanup

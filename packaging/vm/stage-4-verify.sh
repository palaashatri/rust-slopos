#!/usr/bin/env bash
# Stage 4 VM verification harness.
#
# This is a post-install release check. It never treats a checkout or a
# source tree as an installed desktop. Use --dry-run for a non-mutating
# packaging check, or --clean-room with an explicit CARGO_TARGET_DIR to stage
# the real release binaries and session files in a private temporary prefix.
# The clean-room mode deliberately does not claim upgrade, rollback or
# uninstall evidence; those operations require a real package transaction.

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

VM_NAME="${STAGE4_VM_NAME:-unknown}"
PREFIX="${PREFIX:-}"
CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-}"
PREFIX_EXPLICIT=0
TARGET_EXPLICIT=0
DRY_RUN=0
CLEAN_ROOM=0
POSITIONAL_VM=0

if [[ -n "$PREFIX" ]]; then
  PREFIX_EXPLICIT=1
fi
if [[ -n "$CARGO_TARGET_DIR" ]]; then
  TARGET_EXPLICIT=1
fi

usage() {
  cat <<EOF
Usage: $(basename "$0") [VM_NAME] [options]

Verify an installed SLOPOS-I release. Required assets are failures; only
explicitly optional hardware/greeter observations are warnings.

Options:
  --prefix PATH       Verify installed assets below PATH (or PREFIX=PATH).
  --target-dir PATH   Verify release binaries below PATH/release
                      (or CARGO_TARGET_DIR=PATH).
  --dry-run           Validate the packaging dry-run contract only. No
                      install, upgrade, rollback or uninstall is performed.
  --clean-room        Stage the actual release binaries and session files in
                      a private temporary prefix. Requires --target-dir or
                      CARGO_TARGET_DIR. The temporary tree is removed on exit.
  --results-file PATH Write the text result ledger to PATH.
  -h, --help          Show this help.

Exit status:
  0  requested verification passed;
  1  required asset or validation failed;
  2  requested dry-run/clean-room is honest but lifecycle evidence remains
     unverified (never a release-pass result).
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --prefix)
      if [[ $# -lt 2 ]]; then
        echo "--prefix requires a path" >&2
        exit 2
      fi
      PREFIX="$2"
      PREFIX_EXPLICIT=1
      shift 2
      ;;
    --prefix=*)
      PREFIX="${1#--prefix=}"
      PREFIX_EXPLICIT=1
      shift
      ;;
    --target-dir|--cargo-target-dir)
      if [[ $# -lt 2 ]]; then
        echo "$1 requires a path" >&2
        exit 2
      fi
      CARGO_TARGET_DIR="$2"
      TARGET_EXPLICIT=1
      shift 2
      ;;
    --target-dir=*|--cargo-target-dir=*)
      CARGO_TARGET_DIR="${1#*=}"
      TARGET_EXPLICIT=1
      shift
      ;;
    --dry-run)
      DRY_RUN=1
      shift
      ;;
    --clean-room)
      CLEAN_ROOM=1
      shift
      ;;
    --results-file)
      if [[ $# -lt 2 ]]; then
        echo "--results-file requires a path" >&2
        exit 2
      fi
      RESULTS_FILE="$2"
      shift 2
      ;;
    --results-file=*)
      RESULTS_FILE="${1#*=}"
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    --)
      shift
      if [[ $# -gt 0 && "$POSITIONAL_VM" -eq 0 ]]; then
        VM_NAME="$1"
        POSITIONAL_VM=1
        shift
      fi
      if [[ $# -gt 0 ]]; then
        echo "unexpected argument: $1" >&2
        exit 2
      fi
      ;;
    -* )
      echo "unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
    *)
      if [[ "$POSITIONAL_VM" -eq 0 ]]; then
        VM_NAME="$1"
        POSITIONAL_VM=1
        shift
      else
        echo "unexpected argument: $1" >&2
        usage >&2
        exit 2
      fi
      ;;
  esac
done

if [[ "$DRY_RUN" -eq 1 && "$CLEAN_ROOM" -eq 1 ]]; then
  echo "--dry-run and --clean-room are mutually exclusive" >&2
  exit 2
fi
if [[ "$CLEAN_ROOM" -eq 1 && "$PREFIX_EXPLICIT" -eq 1 ]]; then
  echo "--clean-room creates its own prefix; do not combine it with --prefix" >&2
  exit 2
fi

if [[ -z "${RESULTS_FILE:-}" ]]; then
  RESULTS_FILE="/tmp/stage4-results-$(date +%s).txt"
fi

PASSED=0
FAILED=0
WARNINGS=0
UNVERIFIED=0
TEMP_ROOTS=()
CLEAN_ROOM_ROOT=""

cleanup() {
  local root
  for root in "${TEMP_ROOTS[@]}"; do
    if [[ -n "$root" && -d "$root" ]]; then
      rm -rf -- "$root"
    fi
  done
}
trap cleanup EXIT

if ! : >"$RESULTS_FILE"; then
  echo "cannot write result file: $RESULTS_FILE" >&2
  exit 1
fi

if [[ "$CLEAN_ROOM" -eq 1 ]]; then
  if [[ "$TARGET_EXPLICIT" -ne 1 ]]; then
    echo "--clean-room requires --target-dir or CARGO_TARGET_DIR" >&2
    exit 2
  fi
  CLEAN_ROOM_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/slopos-stage4.XXXXXX")"
  TEMP_ROOTS+=("$CLEAN_ROOM_ROOT")
  PREFIX="$CLEAN_ROOM_ROOT/prefix"
  mkdir -p "$PREFIX"
fi

if [[ "$PREFIX_EXPLICIT" -eq 1 || "$CLEAN_ROOM" -eq 1 ]]; then
  case "$PREFIX" in
    /*) ;;
    *)
      echo "PREFIX must be an absolute path: $PREFIX" >&2
      exit 2
      ;;
  esac
fi
if [[ "$TARGET_EXPLICIT" -eq 1 ]]; then
  case "$CARGO_TARGET_DIR" in
    /*) ;;
    *)
      echo "CARGO_TARGET_DIR must be an absolute path: $CARGO_TARGET_DIR" >&2
      exit 2
      ;;
  esac
fi

cat >"$RESULTS_FILE" <<EOF
Stage 4 Verification Results
Generated: $(date -u +%Y-%m-%dT%H:%M:%SZ)
VM: $VM_NAME
Mode: $(if [[ "$DRY_RUN" -eq 1 ]]; then echo dry-run; elif [[ "$CLEAN_ROOM" -eq 1 ]]; then echo clean-room; else echo installed; fi)
Prefix: ${PREFIX:-<system-installed>}
Cargo target: ${CARGO_TARGET_DIR:-<not supplied>}

EOF

log_test() {
  echo -e "${BLUE}[TEST]${NC} $*"
}

log_pass() {
  echo -e "${GREEN}[PASS]${NC} $*"
  PASSED=$((PASSED + 1))
  echo "PASS: $*" >>"$RESULTS_FILE"
}

log_fail() {
  echo -e "${RED}[FAIL]${NC} $*"
  FAILED=$((FAILED + 1))
  echo "FAIL: $*" >>"$RESULTS_FILE"
}

log_optional() {
  echo -e "${YELLOW}[OPTIONAL]${NC} $*"
  WARNINGS=$((WARNINGS + 1))
  echo "OPTIONAL: $*" >>"$RESULTS_FILE"
}

log_unverified() {
  echo -e "${YELLOW}[UNVERIFIED]${NC} $*"
  UNVERIFIED=$((UNVERIFIED + 1))
  echo "UNVERIFIED: $*" >>"$RESULTS_FILE"
}

header() {
  echo ""
  echo -e "${BLUE}========== $* ==========${NC}"
}

check_required_file() {
  local label="$1" path="$2" executable="${3:-0}"
  log_test "Checking $label"
  if [[ ! -f "$path" ]]; then
    log_fail "$label missing: $path"
  elif [[ "$executable" -eq 1 && ! -x "$path" ]]; then
    log_fail "$label is not executable: $path"
  else
    log_pass "$label present"
  fi
}

binary_is_elf() {
  local path="$1" description
  if ! command -v file >/dev/null 2>&1; then
    return 1
  fi
  description="$(file -b "$path" 2>/dev/null || true)"
  [[ "$description" == *ELF* ]]
}

check_binary() {
  local label="$1" path="$2"
  if [[ ! -f "$path" || ! -x "$path" ]]; then
    log_fail "$label missing or not executable: $path"
  elif ! binary_is_elf "$path"; then
    log_fail "$label is not a verifiable ELF executable: $path"
  else
    log_pass "$label verified: $path"
  fi
}

RELEASE_BINARIES=(
  slopos-session
  slopos-compositor
  slopos-shell
  finder
  settings
  textedit
  terminal
  appstore
)

SESSION_RELATIVE_FILES=(
  share/wayland-sessions/slopos-i.desktop
  share/xsessions/slopos-i.desktop
  bin/start-slopos-i
  lib/systemd/user/slopos-i.service
)

check_target_release() {
  local release_dir="$CARGO_TARGET_DIR/release" name
  if [[ "$TARGET_EXPLICIT" -ne 1 ]]; then
    return 0
  fi
  header "Shared Cargo release target verification"
  if [[ ! -d "$release_dir" ]]; then
    log_fail "release target directory missing: $release_dir"
    return 0
  fi
  for name in "${RELEASE_BINARIES[@]}"; do
    check_binary "release/$name" "$release_dir/$name"
  done
}

check_prefix_assets() {
  local prefix="$1" name
  header "Installed prefix asset verification"
  if [[ ! -d "$prefix" ]]; then
    log_fail "installed prefix missing: $prefix"
    return 0
  fi
  for name in "${RELEASE_BINARIES[@]}"; do
    check_binary "$prefix/bin/$name" "$prefix/bin/$name"
  done
  check_required_file "$prefix/share/wayland-sessions/slopos-i.desktop" \
    "$prefix/share/wayland-sessions/slopos-i.desktop"
  check_required_file "$prefix/share/xsessions/slopos-i.desktop" \
    "$prefix/share/xsessions/slopos-i.desktop"
  check_required_file "$prefix/bin/start-slopos-i" \
    "$prefix/bin/start-slopos-i" 1
  check_required_file "$prefix/lib/systemd/user/slopos-i.service" \
    "$prefix/lib/systemd/user/slopos-i.service"
}

check_system_assets() {
  local name path found_wayland=0 found_xsession=0 session_path
  header "Installed system asset verification"
  for name in "${RELEASE_BINARIES[@]}"; do
    path="$(command -v "$name" 2>/dev/null || true)"
    if [[ -n "$path" ]]; then
      check_binary "$name in PATH" "$path"
    else
      log_fail "$name not found in PATH"
    fi
  done
  for session_path in \
    /usr/local/share/wayland-sessions/slopos-i.desktop \
    /usr/share/wayland-sessions/slopos-i.desktop \
    "${XDG_DATA_HOME:-$HOME/.local/share}/wayland-sessions/slopos-i.desktop"; do
    if [[ -f "$session_path" ]]; then
      check_required_file "Wayland session file" "$session_path"
      found_wayland=1
      break
    fi
  done
  if [[ "$found_wayland" -eq 0 ]]; then
    log_fail "slopos-i.desktop not found in installed Wayland session locations"
  fi
  for session_path in \
    /usr/local/share/xsessions/slopos-i.desktop \
    /usr/share/xsessions/slopos-i.desktop \
    "${XDG_DATA_HOME:-$HOME/.local/share}/xsessions/slopos-i.desktop"; do
    if [[ -f "$session_path" ]]; then
      check_required_file "X11 session file" "$session_path"
      found_xsession=1
      break
    fi
  done
  if [[ "$found_xsession" -eq 0 ]]; then
    log_fail "slopos-i.desktop not found in installed X11 session locations"
  fi
  if [[ ! -f /usr/local/bin/start-slopos-i && ! -f /usr/bin/start-slopos-i && \
        ! -f "${XDG_BIN_HOME:-$HOME/.local/bin}/start-slopos-i" ]]; then
    log_fail "start-slopos-i not found in installed locations"
  else
    log_pass "start-slopos-i installed"
  fi
  if [[ ! -f /usr/local/lib/systemd/user/slopos-i.service && \
        ! -f /usr/lib/systemd/user/slopos-i.service && \
        ! -f "${XDG_DATA_HOME:-$HOME/.local/share}/systemd/user/slopos-i.service" ]]; then
    log_fail "slopos-i.service not found in installed locations"
  else
    log_pass "slopos-i.service installed"
  fi
}

dependency_present() {
  local name candidate
  name="$1"
  shift
  if command -v pkg-config >/dev/null 2>&1; then
    for candidate in "$@"; do
      if pkg-config --exists "$candidate" 2>/dev/null; then
        return 0
      fi
    done
  fi
  if command -v dpkg-query >/dev/null 2>&1; then
    for candidate in "$@"; do
      if dpkg-query -W -f='${Status}' "$candidate" 2>/dev/null | \
        grep -q 'install ok installed'; then
        return 0
      fi
    done
  fi
  if command -v pacman >/dev/null 2>&1; then
    for candidate in "$@"; do
      if pacman -Q "$candidate" >/dev/null 2>&1; then
        return 0
      fi
    done
  fi
  echo "dependency probe failed: $name" >&2
  return 1
}

check_required_dependencies() {
  header "Required runtime dependency verification"
  if [[ "$DRY_RUN" -eq 1 ]]; then
    log_unverified "runtime dependencies skipped in dry-run mode"
    return 0
  fi
  if dependency_present wayland wayland-client libwayland-client0 libwayland-dev wayland; then
    log_pass "wayland runtime present"
  else
    log_fail "wayland runtime dependency missing"
  fi
  if dependency_present libdrm libdrm libdrm2 libdrm-dev; then
    log_pass "libdrm runtime present"
  else
    log_fail "libdrm runtime dependency missing"
  fi
  if dependency_present mesa gbm egl libgbm1 mesa mesa-libgbm; then
    log_pass "mesa/GBM runtime present"
  else
    log_fail "mesa/GBM runtime dependency missing"
  fi
  if dependency_present seatd libseat seatd libseat2 libseat-dev; then
    log_pass "seat acquisition runtime present"
  else
    log_fail "seatd/libseat runtime dependency missing"
  fi
  if dependency_present libinput libinput libinput10 libinput-dev; then
    log_pass "libinput runtime present"
  else
    log_fail "libinput runtime dependency missing"
  fi
}

check_optional_hardware() {
  header "Optional hardware and greeter observations"
  if [[ -e /dev/dri/card0 || -e /dev/dri/renderD128 ]]; then
    log_pass "optional DRM device is visible"
  else
    log_optional "no DRM device visible in this VM (hardware/runtime observation only)"
  fi
  if [[ -f /etc/greetd/config.toml ]]; then
    if grep -q 'tuigreet' /etc/greetd/config.toml && \
       grep -q 'start-slopos-i' /etc/greetd/config.toml; then
      log_pass "optional greetd configuration points at SLOPOS-I"
    else
      log_optional "greetd is present but not configured for SLOPOS-I"
    fi
  else
    log_optional "greetd is not configured (optional greeter path)"
  fi
}

check_static_packaging() {
  header "Installer and packaging source verification"
  check_required_file "install.sh" "$REPO_ROOT/install.sh" 1
  log_test "Checking install.sh syntax"
  if bash -n "$REPO_ROOT/install.sh" 2>/dev/null; then
    log_pass "install.sh syntax is valid"
  else
    log_fail "install.sh has syntax errors"
  fi
  log_test "Checking install.sh session wiring and locked build"
  if grep -q 'install-session-files.sh' "$REPO_ROOT/install.sh" && \
     grep -q 'os-release' "$REPO_ROOT/install.sh" && \
     grep -q 'cargo build --release --workspace --locked' "$REPO_ROOT/install.sh" && \
     grep -q 'CARGO_TARGET_DIR' "$REPO_ROOT/install.sh"; then
    log_pass "install.sh has session wiring, locked build and shared target support"
  else
    log_fail "install.sh is missing required release wiring"
  fi

  check_required_file "scripts/install-session-files.sh" \
    "$REPO_ROOT/scripts/install-session-files.sh" 1
  log_test "Checking install-session-files.sh syntax"
  if bash -n "$REPO_ROOT/scripts/install-session-files.sh" 2>/dev/null; then
    log_pass "install-session-files.sh syntax is valid"
  else
    log_fail "install-session-files.sh has syntax errors"
  fi

  check_required_file "packaging/arch/PKGBUILD" "$REPO_ROOT/packaging/arch/PKGBUILD"
  log_test "Checking PKGBUILD locked build and release binaries"
  if grep -q 'cargo build --release --workspace --locked' "$REPO_ROOT/packaging/arch/PKGBUILD" && \
     grep -q 'CARGO_TARGET_DIR' "$REPO_ROOT/packaging/arch/PKGBUILD" && \
     grep -q '^pkgname=slopos-i' "$REPO_ROOT/packaging/arch/PKGBUILD"; then
    log_pass "PKGBUILD defines locked shared-target build"
  else
    log_fail "PKGBUILD missing locked build or target wiring"
  fi

  check_required_file "packaging/debian/control" "$REPO_ROOT/packaging/debian/control"
  check_required_file "packaging/debian/rules" "$REPO_ROOT/packaging/debian/rules" 1
  log_test "Checking debian/rules locked build and target wiring"
  if grep -q 'cargo build --release --workspace --locked' "$REPO_ROOT/packaging/debian/rules" && \
     grep -q 'CARGO_TARGET_DIR' "$REPO_ROOT/packaging/debian/rules"; then
    log_pass "debian/rules defines locked shared-target build"
  else
    log_fail "debian/rules missing locked build or target wiring"
  fi

  check_required_file "packaging/iso/packages.x86_64" "$REPO_ROOT/packaging/iso/packages.x86_64"
  check_required_file "packaging/iso/build-iso.sh" "$REPO_ROOT/packaging/iso/build-iso.sh" 1
  check_required_file "packaging/iso/profiledef.sh" "$REPO_ROOT/packaging/iso/profiledef.sh"
}

run_session_dry_run_contract() {
  local probe_root probe_prefix dry_log expected
  header "Non-mutating packaging dry-run contract"
  probe_root="$(mktemp -d "${TMPDIR:-/tmp}/slopos-stage4-dry-run.XXXXXX")"
  TEMP_ROOTS+=("$probe_root")
  probe_prefix="$probe_root/prefix"
  dry_log="$probe_root/install-session-files.log"
  if "$REPO_ROOT/scripts/install-session-files.sh" --dry-run --prefix "$probe_prefix" >"$dry_log" 2>&1; then
    log_pass "session installer dry-run command succeeded"
  else
    log_fail "session installer dry-run command failed"
    return 0
  fi
  if [[ ! -e "$probe_prefix" ]]; then
    log_pass "session installer dry-run wrote no files"
  else
    log_fail "session installer dry-run created files under $probe_prefix"
  fi
  for expected in "${SESSION_RELATIVE_FILES[@]}"; do
    if grep -Fq "$probe_prefix/$expected" "$dry_log"; then
      log_pass "dry-run plan contains $expected"
    else
      log_fail "dry-run plan omits $expected"
    fi
  done
  log_unverified "session-file install was planned only; no installed state is claimed"
  log_unverified "upgrade lifecycle has no package transaction runner"
  log_unverified "rollback lifecycle has no package transaction runner"
  log_unverified "uninstall lifecycle has no package transaction runner"
}

stage_clean_room_install() {
  local release_dir="$CARGO_TARGET_DIR/release" name clean_log
  header "Clean-room release staging contract"
  mkdir -p "$PREFIX/bin"
  for name in "${RELEASE_BINARIES[@]}"; do
    if ! install -Dm755 "$release_dir/$name" "$PREFIX/bin/$name"; then
      log_fail "could not stage release/$name in clean-room prefix"
      return 0
    fi
  done
  clean_log="$CLEAN_ROOM_ROOT/session-install.log"
  if "$REPO_ROOT/scripts/install-session-files.sh" --prefix "$PREFIX" >"$clean_log" 2>&1; then
    log_pass "session files staged in private clean-room prefix"
  else
    log_fail "session files could not be staged in clean-room prefix"
    cat "$clean_log" >&2 || true
  fi
  check_prefix_assets "$PREFIX"
  log_pass "clean-room install assets staged under private prefix $PREFIX"
  log_unverified "upgrade was not executed; no upgrade transaction runner exists"
  log_unverified "rollback was not executed; no rollback transaction runner exists"
  log_unverified "uninstall was not executed; no uninstall transaction runner exists"
}

header "Task 4.0: Verification mode"
if [[ "$DRY_RUN" -eq 1 ]]; then
  log_pass "dry-run mode selected; no installed-state claim will be made"
elif [[ "$CLEAN_ROOM" -eq 1 ]]; then
  log_pass "clean-room mode selected; writes are confined to a temporary prefix"
else
  log_pass "installed-release mode selected"
fi

check_static_packaging
check_target_release

if [[ "$DRY_RUN" -eq 1 ]]; then
  run_session_dry_run_contract
else
  check_required_dependencies
  if [[ "$CLEAN_ROOM" -eq 1 ]]; then
    stage_clean_room_install
  elif [[ "$PREFIX_EXPLICIT" -eq 1 ]]; then
    check_prefix_assets "$PREFIX"
  else
    check_system_assets
  fi
  check_optional_hardware
fi

header "Summary"
TOTAL=$((PASSED + FAILED + WARNINGS + UNVERIFIED))
echo ""
echo "Results written to: $RESULTS_FILE"
echo ""
echo -e "  ${GREEN}Passed:${NC}      $PASSED"
echo -e "  ${RED}Failed:${NC}      $FAILED"
echo -e "  ${YELLOW}Optional:${NC}    $WARNINGS"
echo -e "  ${YELLOW}Unverified:${NC}  $UNVERIFIED"
echo -e "  ${BLUE}Total:${NC}       $TOTAL"
echo ""

if [[ "$FAILED" -gt 0 ]]; then
  echo -e "${RED}Stage 4 verification FAILED${NC}"
  cat "$RESULTS_FILE"
  exit 1
fi
if [[ "$UNVERIFIED" -gt 0 ]]; then
  echo -e "${YELLOW}Stage 4 produced no release pass: lifecycle evidence is UNVERIFIED${NC}"
  cat "$RESULTS_FILE"
  exit 2
fi

echo -e "${GREEN}Stage 4 installed-asset verification PASSED${NC}"
echo "This result covers the requested asset checks only; it is not a claim of"
echo "live display-manager login, package upgrade, rollback or uninstall evidence."
exit 0

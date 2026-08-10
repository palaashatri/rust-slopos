#!/usr/bin/env bash
# Stage 4 VM Verification Harness
# Run this script on the VM to verify the complete distribution chain.
# Exit code 0 = all tests pass, non-zero = failures

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
VM_NAME="${1:-unknown}"
RESULTS_FILE="/tmp/stage4-results-$(date +%s).txt"

# Counters
PASSED=0
FAILED=0
WARNINGS=0

# Logging
log_test() {
  echo -e "${BLUE}[TEST]${NC} $*"
}

log_pass() {
  echo -e "${GREEN}[PASS]${NC} $*"
  ((PASSED++))
  echo "PASS: $*" >> "$RESULTS_FILE"
}

log_fail() {
  echo -e "${RED}[FAIL]${NC} $*"
  ((FAILED++))
  echo "FAIL: $*" >> "$RESULTS_FILE"
}

log_warn() {
  echo -e "${YELLOW}[WARN]${NC} $*"
  ((WARNINGS++))
  echo "WARN: $*" >> "$RESULTS_FILE"
}

header() {
  echo ""
  echo -e "${BLUE}========== $* ==========${NC}"
}

# Initialize results file
cat > "$RESULTS_FILE" <<EOF
Stage 4 Verification Results
Generated: $(date)
VM: $VM_NAME

EOF

# ============================================================================
# Task 4.0 — Re-ground verification
# ============================================================================

header "Task 4.0: Re-ground verification"

log_test "Checking for slopos-i.desktop"
if [ -f /usr/local/share/wayland-sessions/slopos-i.desktop ] || \
   [ -f /usr/share/wayland-sessions/slopos-i.desktop ]; then
  log_pass "slopos-i.desktop found"
else
  log_fail "slopos-i.desktop not found"
fi

log_test "Checking for slopos-compositor binary"
if command -v slopos-compositor &>/dev/null; then
  log_pass "slopos-compositor in PATH"
else
  log_fail "slopos-compositor not found in PATH"
fi

log_test "Checking for slopos-shell binary"
if command -v slopos-shell &>/dev/null; then
  log_pass "slopos-shell in PATH"
else
  log_fail "slopos-shell not found in PATH"
fi

# ============================================================================
# Task 4.1 — Dependencies verification
# ============================================================================

header "Task 4.1: Dependencies verification"

# Check a sample of critical deps
DEPS_TO_CHECK=("wayland" "libdrm" "mesa" "seatd" "libinput")

for dep in "${DEPS_TO_CHECK[@]}"; do
  log_test "Checking for $dep"
  if pkg-config --exists "$dep" 2>/dev/null || \
     dpkg -l | grep -q "$dep" 2>/dev/null || \
     pacman -Q "$dep" &>/dev/null 2>&1; then
    log_pass "$dep installed"
  else
    log_warn "$dep not found (may be installed under different name)"
  fi
done

# ============================================================================
# Task 4.2 — Layered installer verification
# ============================================================================

header "Task 4.2: Layered installer verification"

log_test "Checking install.sh exists and is executable"
if [ -x "$REPO_ROOT/install.sh" ]; then
  log_pass "install.sh is executable"
else
  log_fail "install.sh not found or not executable"
fi

log_test "Checking install.sh syntax"
if bash -n "$REPO_ROOT/install.sh" 2>/dev/null; then
  log_pass "install.sh syntax is valid"
else
  log_fail "install.sh has syntax errors"
fi

log_test "Checking install.sh for required functions"
if grep -q 'install-session-files.sh' "$REPO_ROOT/install.sh" && \
   grep -q 'os-release' "$REPO_ROOT/install.sh"; then
  log_pass "install.sh has session files wiring"
else
  log_fail "install.sh missing session files integration"
fi

# ============================================================================
# Task 4.3 — PKGBUILD verification
# ============================================================================

header "Task 4.3: PKGBUILD verification"

log_test "Checking PKGBUILD exists"
if [ -f "$REPO_ROOT/packaging/arch/PKGBUILD" ]; then
  log_pass "PKGBUILD exists"
else
  log_fail "PKGBUILD not found"
  exit 1
fi

log_test "Checking PKGBUILD for slopos-i package"
if grep -q '^pkgname=slopos-i' "$REPO_ROOT/packaging/arch/PKGBUILD"; then
  log_pass "PKGBUILD defines slopos-i package"
else
  log_fail "PKGBUILD does not define slopos-i package"
fi

log_test "Checking PKGBUILD for build function"
if grep -q 'cargo build --release --workspace' "$REPO_ROOT/packaging/arch/PKGBUILD"; then
  log_pass "PKGBUILD has cargo build command"
else
  log_fail "PKGBUILD missing cargo build"
fi

# ============================================================================
# Task 4.4 — .deb packaging verification
# ============================================================================

header "Task 4.4: .deb packaging verification"

log_test "Checking debian/control exists"
if [ -f "$REPO_ROOT/packaging/debian/control" ]; then
  log_pass "debian/control exists"
else
  log_fail "debian/control not found"
fi

log_test "Checking debian/rules is executable"
if [ -x "$REPO_ROOT/packaging/debian/rules" ]; then
  log_pass "debian/rules is executable"
else
  log_fail "debian/rules not executable"
fi

log_test "Checking debian/rules for build command"
if grep -q 'cargo build --release --workspace' "$REPO_ROOT/packaging/debian/rules"; then
  log_pass "debian/rules has cargo build"
else
  log_fail "debian/rules missing cargo build"
fi

# ============================================================================
# Task 4.7 — archiso profile verification
# ============================================================================

header "Task 4.7: archiso profile verification"

log_test "Checking ISO profile exists"
if [ -f "$REPO_ROOT/packaging/iso/packages.x86_64" ]; then
  log_pass "ISO packages.x86_64 exists"
else
  log_fail "ISO packages.x86_64 not found"
fi

log_test "Checking ISO build script"
if [ -x "$REPO_ROOT/packaging/iso/build-iso.sh" ]; then
  log_pass "build-iso.sh is executable"
else
  log_fail "build-iso.sh not executable"
fi

log_test "Checking profiledef.sh"
if [ -f "$REPO_ROOT/packaging/iso/profiledef.sh" ]; then
  log_pass "profiledef.sh exists"
else
  log_fail "profiledef.sh not found"
fi

# ============================================================================
# Runtime tests (only if installer was actually run)
# ============================================================================

header "Task 4.5–4.6: Runtime verification (if installed)"

log_test "Checking if slopos-compositor can be found"
if command -v slopos-compositor &>/dev/null; then
  log_pass "slopos-compositor executable available"

  # Verify it's the right binary (check for symbol, not execute)
  if file "$(which slopos-compositor)" | grep -q 'ELF.*executable'; then
    log_pass "slopos-compositor is an ELF binary"
  else
    log_warn "slopos-compositor may not be a proper binary"
  fi
else
  log_warn "slopos-compositor not installed (expected if this is pre-install)"
fi

log_test "Checking if slopos-shell can be found"
if command -v slopos-shell &>/dev/null; then
  log_pass "slopos-shell executable available"
else
  log_warn "slopos-shell not installed (expected if this is pre-install)"
fi

# ============================================================================
# Session file checks
# ============================================================================

header "Session file verification"

WAYLAND_SESSION_PATHS=(
  "/usr/local/share/wayland-sessions"
  "/usr/share/wayland-sessions"
  "$HOME/.local/share/wayland-sessions"
)

FOUND_SESSION=0
for path in "${WAYLAND_SESSION_PATHS[@]}"; do
  if [ -f "$path/slopos-i.desktop" ]; then
    log_pass "Found slopos-i.desktop at $path"
    FOUND_SESSION=1
    break
  fi
done

if [ $FOUND_SESSION -eq 0 ]; then
  log_warn "No slopos-i.desktop found in standard locations (may be installed)"
fi

# ============================================================================
# Greetd configuration (if --with-greeter was used)
# ============================================================================

header "Greeter configuration (if enabled)"

if [ -f /etc/greetd/config.toml ]; then
  if grep -q 'tuigreet' /etc/greetd/config.toml && \
     grep -q 'start-slopos-i' /etc/greetd/config.toml; then
    log_pass "greetd configured for SLOPOS-I"
  else
    log_warn "greetd config exists but may not be for SLOPOS-I"
  fi
else
  log_warn "greetd not configured (expected if --with-greeter not used)"
fi

# ============================================================================
# Summary
# ============================================================================

header "Summary"

TOTAL=$((PASSED + FAILED + WARNINGS))
echo ""
echo "Results written to: $RESULTS_FILE"
echo ""
echo -e "  ${GREEN}Passed:${NC}  $PASSED"
echo -e "  ${RED}Failed:${NC}  $FAILED"
echo -e "  ${YELLOW}Warnings:${NC} $WARNINGS"
echo -e "  ${BLUE}Total:${NC}   $TOTAL"
echo ""

if [ $FAILED -eq 0 ]; then
  echo -e "${GREEN}✓ All critical tests passed${NC}"
  echo ""
  echo "Next steps:"
  echo "  1. If binaries not found, run: sudo bash $REPO_ROOT/install.sh"
  echo "  2. To use greeter, add: --with-greeter"
  echo "  3. After install, reboot or restart login manager"
  echo "  4. Select 'SLOPOS-I' from session menu"
  echo ""
  exit 0
else
  echo -e "${RED}✗ Some tests failed${NC}"
  echo ""
  cat "$RESULTS_FILE"
  exit 1
fi
